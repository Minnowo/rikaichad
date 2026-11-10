// Ported from Yomitan ext/js/language/translator.js (findTerms,
// _findTermsInternal, _getDeinflections,
// _getDictionaryDeinflections, _addEntriesToDeinflections,
// _groupDeinflectionsByTerm, _matchEntriesToDeinflections,
// _getDictionaryEntries, _mergeInflectionRuleChains,
// _mergeTextProcessorRuleChains, _areArraysEqualIgnoreOrder,
// _getJapaneseChineseKoreanOnlyText,
// _createTermDictionaryEntryFromDatabaseEntry, _addUserFacingInflections,
// _sortTermDictionaryEntryDefinitions, _sortTermDictionaryEntrySimpleData)
// at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Term lookup: candidates -> matched dictionary rows -> entries, grouped
//! by the result mode, sorted, with tags and metadata.

use std::collections::{HashMap, HashSet};

use jisho_ja::{
    condition_flags_from_parts_of_speech, conditions_match, is_code_point_chinese,
    is_code_point_japanese, is_code_point_korean, user_facing_rule,
};
use jisho_store::{BulkHit, MatchSource, RawGlossary, Store, StoreError, TermHit};
use serde_json::Value;

use crate::deinflection::{
    ChainTables, Deinflection, algorithm_deinflections, identity_deinflection,
};
use crate::entry::{
    Definition, FindTermsResult, Headword, InflectionChain, InflectionSource, RuleName, Source,
    TermEntry,
};
use crate::grouping::{group_by_headword, group_by_term, merge, remove_excluded_definitions};
use crate::meta::{HeadwordOrder, MetaKinds, add_term_meta, update_sort_frequencies};
use crate::options::{EnabledDictionaries, FindTermsOptions, ResultMode};
use crate::sort::{sort_definitions, sort_simple_data, sort_term_entries};
use crate::tags::{TagLookup, TagRegistry, flag_redundant_definition_tags};
use crate::utf16::utf16_len;

/// Finds the dictionary entries for the start of `text`, like Yomitan's
/// findTerms in the result mode of `options`, sorted, at most
/// `options.limit` of them. Glossaries are not loaded; see
/// [`load_glossaries`].
pub fn find_terms(
    store: &Store,
    text: &str,
    dictionaries: &EnabledDictionaries,
    options: &FindTermsOptions,
) -> Result<FindTermsResult, StoreError> {
    let dictionaries = dictionaries.for_mode(options.mode);
    let text = if options.remove_non_japanese_characters {
        japanese_chinese_korean_prefix(text)
    } else {
        text
    };
    if text.is_empty() {
        return Ok(FindTermsResult {
            entries: Vec::new(),
            original_text_length: 0,
            truncated: false,
        });
    }
    let mut registry = TagRegistry::default();
    let mut tables = ChainTables::default();
    let mut truncated = false;
    let deinflections = deinflections(
        store,
        text,
        &dictionaries,
        options,
        &mut tables,
        &mut truncated,
    )?;
    let (entries, original_text_length) = dictionary_entries(
        deinflections,
        &dictionaries,
        options,
        &mut tables,
        &mut registry,
    );
    let primary_reading = options.primary_reading.as_str();
    let mut entries = match options.mode {
        ResultMode::Group => group_by_headword(entries, primary_reading),
        ResultMode::Term => group_by_term(entries, primary_reading),
        ResultMode::Merge => merge(
            store,
            entries,
            &dictionaries,
            primary_reading,
            &mut registry,
        )?,
        ResultMode::Split | ResultMode::Simple => entries,
    };
    if let Some(excluded) = dictionaries.excluded {
        remove_excluded_definitions(&mut entries, excluded);
    }

    // Frequency orders first: they are the only metadata the sort uses.
    // Tags and metadata do not affect the order otherwise, so they are
    // computed only for the entries kept (same result as Yomitan computing
    // them for all, then cutting at maxResults).
    if let Some(sort_dictionary) = dictionaries.sort_frequency_dictionary {
        if dictionaries.by_id.contains_key(&sort_dictionary) {
            add_term_meta(
                store,
                &mut entries,
                &dictionaries,
                &[sort_dictionary],
                MetaKinds::FrequenciesOnly,
                None,
                &mut registry,
            )?;
        }
        update_sort_frequencies(
            &mut entries,
            sort_dictionary,
            dictionaries.sort_frequency_order,
        );
        for entry in &mut entries {
            entry.frequencies.clear();
        }
    }
    let headword_order = HeadwordOrder::of(&entries);
    if entries.len() > 1 {
        sort_term_entries(&mut entries);
    }
    if let Some(limit) = options.limit {
        entries.truncate(limit);
    }

    if options.mode != ResultMode::Simple {
        add_term_meta(
            store,
            &mut entries,
            &dictionaries,
            &dictionaries.ids(),
            MetaKinds::All,
            Some(&headword_order),
            &mut registry,
        )?;
        expand_tags(store, &mut entries, &registry)?;
    } else if let Some(sort_dictionary) = dictionaries.sort_frequency_dictionary
        && dictionaries.by_id.contains_key(&sort_dictionary)
    {
        add_term_meta(
            store,
            &mut entries,
            &dictionaries,
            &[sort_dictionary],
            MetaKinds::All,
            Some(&headword_order),
            &mut registry,
        )?;
    }
    for entry in &mut entries {
        flag_redundant_definition_tags(&mut entry.definitions);
        sort_definitions(&mut entry.definitions);
        sort_simple_data(&mut entry.frequencies, |f| {
            (f.headword_index, f.dictionary_index, f.index)
        });
        sort_simple_data(&mut entry.pronunciations, |p| {
            (p.headword_index, p.dictionary_index, p.index)
        });
        add_user_facing_inflections(entry);
    }
    Ok(FindTermsResult {
        entries,
        original_text_length,
        truncated,
    })
}

/// _expandTagGroupsAndGroup for term entries: every tag group becomes tags.
fn expand_tags(
    store: &Store,
    entries: &mut [TermEntry],
    registry: &TagRegistry,
) -> Result<(), StoreError> {
    let groups = entries.iter().flat_map(|e| {
        let headwords = e.headwords.iter().flat_map(|h| &h.tag_groups);
        let definitions = e.definitions.iter().flat_map(|d| &d.tag_groups);
        let pronunciations = e
            .pronunciations
            .iter()
            .flat_map(|p| &p.pronunciations)
            .flat_map(|p| &p.tag_groups);
        headwords.chain(definitions).chain(pronunciations)
    });
    let lookup = TagLookup::load(store, groups)?;
    for entry in entries {
        for headword in &mut entry.headwords {
            headword.tags = lookup.expand(registry, &headword.tag_groups);
        }
        for definition in &mut entry.definitions {
            definition.tags = lookup.expand(registry, &definition.tag_groups);
        }
        for pronunciation in entry
            .pronunciations
            .iter_mut()
            .flat_map(|p| &mut p.pronunciations)
        {
            pronunciation.tags = lookup.expand(registry, &pronunciation.tag_groups);
        }
    }
    Ok(())
}

/// Loads the glossaries of the definitions in `entries` (parsed, into
/// `entries`), dropping deinflection-reference items as Yomitan does, until
/// `max_bytes` of stored glossary is used. Returns how many leading entries
/// were filled completely; later entries keep empty glossaries and should
/// be dropped by the caller.
pub fn load_glossaries(
    store: &Store,
    entries: &mut [TermEntry],
    max_bytes: usize,
) -> Result<usize, StoreError> {
    fill_glossaries(store, entries, max_bytes, |definition, raw| {
        let mut items: Vec<Value> =
            serde_json::from_str(&raw.json).map_err(|e| corrupt(definition.id, &e))?;
        let keep = definition.raw_entries;
        items.retain(|item| keep || !item.is_array());
        definition.entries = items;
        Ok(())
    })
}

/// Like [`load_glossaries`], but fills each definition's `glossary` with
/// the stored JSON texts, unparsed (only rows whose deinflection references
/// must be dropped are parsed). For responses that copy them as they are.
pub fn load_raw_glossaries(
    store: &Store,
    entries: &mut [TermEntry],
    max_bytes: usize,
) -> Result<usize, StoreError> {
    fill_glossaries(store, entries, max_bytes, |definition, raw| {
        definition.glossary = Some(if definition.raw_entries || !definition.has_form_of {
            raw
        } else {
            without_references(definition.id, &raw)?
        });
        Ok(())
    })
}

/// Loads the glossaries of `entries` within `max_bytes` and hands each
/// definition its stored glossary. Returns how many leading entries were
/// filled completely.
fn fill_glossaries(
    store: &Store,
    entries: &mut [TermEntry],
    max_bytes: usize,
    mut fill: impl FnMut(&mut Definition, RawGlossary) -> Result<(), StoreError>,
) -> Result<usize, StoreError> {
    let ids: Vec<i64> = entries
        .iter()
        .flat_map(|e| e.definitions.iter().map(|d| d.id))
        .collect();
    let (mut glossaries, _) = store.glossaries_within(&ids, max_bytes)?;
    // A row can back several entries in merge mode: the last use takes the
    // glossary, earlier ones copy it.
    let mut uses: HashMap<i64, usize> = HashMap::new();
    for id in &ids {
        *uses.entry(*id).or_insert(0) += 1;
    }
    let mut filled = 0;
    for entry in entries.iter_mut() {
        let mut complete = true;
        for definition in &mut entry.definitions {
            let remaining = uses.get_mut(&definition.id).map_or(0, |n| {
                *n = n.saturating_sub(1);
                *n
            });
            let raw = if remaining == 0 {
                glossaries.remove(&definition.id)
            } else {
                glossaries.get(&definition.id).cloned()
            };
            match raw {
                Some(raw) => fill(definition, raw)?,
                None => complete = false,
            }
        }
        if !complete {
            break;
        }
        filled += 1;
    }
    Ok(filled)
}

/// The glossary without its deinflection-reference items (and their text).
fn without_references(id: i64, raw: &RawGlossary) -> Result<RawGlossary, StoreError> {
    let items: Vec<Value> = serde_json::from_str(&raw.json).map_err(|e| corrupt(id, &e))?;
    let texts: Vec<Value> = serde_json::from_str(&raw.text).map_err(|e| corrupt(id, &e))?;
    let (items, texts): (Vec<Value>, Vec<Value>) = items
        .into_iter()
        .zip(texts)
        .filter(|(item, _)| !item.is_array())
        .unzip();
    Ok(RawGlossary {
        json: serde_json::to_string(&items).map_err(|e| corrupt(id, &e))?,
        text: serde_json::to_string(&texts).map_err(|e| corrupt(id, &e))?,
    })
}

fn corrupt(id: i64, error: &serde_json::Error) -> StoreError {
    StoreError::Corrupt {
        message: format!("glossary of term {id}: {error}"),
    }
}

/// _getDeinflections: candidates with their matched rows, plus the
/// dictionary's own deinflection references, without rows that only hold
/// references.
fn deinflections(
    store: &Store,
    text: &str,
    dictionaries: &EnabledDictionaries,
    options: &FindTermsOptions,
    tables: &mut ChainTables,
    truncated: &mut bool,
) -> Result<Vec<Deinflection>, StoreError> {
    let mut deinflections = if options.deinflect {
        algorithm_deinflections(text, tables)
    } else {
        vec![identity_deinflection(text, tables)]
    };
    if deinflections.is_empty() {
        return Ok(Vec::new());
    }
    *truncated |= add_entries(store, &mut deinflections, dictionaries, options)?;
    let mut from_dictionary = dictionary_deinflections(&deinflections, dictionaries, tables);
    *truncated |= add_entries(store, &mut from_dictionary, dictionaries, options)?;
    deinflections.extend(from_dictionary);

    for deinflection in &mut deinflections {
        deinflection
            .entries
            .retain(|entry| entry.hit.content_count > 0);
    }
    deinflections.retain(|d| !d.entries.is_empty());
    Ok(deinflections)
}

/// _getDictionaryDeinflections: a new candidate for each deinflection
/// reference (`[formOf, rules]` glossary item) of each matched row, its
/// rules appended to the algorithm's.
fn dictionary_deinflections(
    deinflections: &[Deinflection],
    dictionaries: &EnabledDictionaries,
    tables: &mut ChainTables,
) -> Vec<Deinflection> {
    let mut result = Vec::new();
    for deinflection in deinflections {
        for entry in &deinflection.entries {
            let use_deinflections = dictionaries
                .by_id
                .get(&entry.hit.dictionary_id)
                .is_none_or(|d| d.use_deinflections);
            if !use_deinflections {
                continue;
            }
            for (form_of, rules) in &entry.hit.form_of {
                if form_of.is_empty() {
                    continue;
                }
                let chains = tables
                    .inflections(deinflection.inflections)
                    .iter()
                    .map(|chain| InflectionChain {
                        source: if chain.inflection_rules.is_empty() {
                            InflectionSource::Dictionary
                        } else {
                            InflectionSource::Both
                        },
                        inflection_rules: chain
                            .inflection_rules
                            .iter()
                            .cloned()
                            .chain(rules.iter().map(|r| RuleName {
                                name: r.clone(),
                                description: None,
                            }))
                            .collect(),
                    })
                    .collect();
                let inflections = tables.add_inflections(chains);
                result.push(Deinflection {
                    original_text: deinflection.original_text.clone(),
                    transformed_text: deinflection.transformed_text.clone(),
                    deinflected_text: form_of.clone(),
                    conditions: 0,
                    // Shared with the parent, as in Yomitan.
                    text_processors: deinflection.text_processors,
                    inflections,
                    entries: Vec::new(),
                });
            }
        }
    }
    result
}

/// _addEntriesToDeinflections + _matchEntriesToDeinflections: one bulk
/// lookup of the distinct deinflected texts; each row goes to every
/// candidate of the text that claimed it whose conditions fit the row's
/// parts of speech. Returns whether the matched-rows cap left rows out.
fn add_entries(
    store: &Store,
    deinflections: &mut [Deinflection],
    dictionaries: &EnabledDictionaries,
    options: &FindTermsOptions,
) -> Result<bool, StoreError> {
    let mut terms: Vec<String> = Vec::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for (i, deinflection) in deinflections.iter().enumerate() {
        let text = &deinflection.deinflected_text;
        match positions.get(text) {
            Some(&group) => {
                if let Some(members) = groups.get_mut(group) {
                    members.push(i);
                }
            }
            None => {
                positions.insert(text.clone(), terms.len());
                terms.push(text.clone());
                groups.push(vec![i]);
            }
        }
    }
    if terms.is_empty() {
        return Ok(false);
    }
    let (rows, truncated) =
        store.find_terms_bulk(&terms, dictionaries.extra_searched(), options.max_hits)?;
    for bulk in rows {
        let filter = dictionaries
            .by_id
            .get(&bulk.hit.dictionary_id)
            .is_none_or(|d| d.parts_of_speech_filter);
        let definition_conditions = condition_flags_from_parts_of_speech(&bulk.hit.rules);
        let Some(members) = groups.get(bulk.item_index) else {
            continue;
        };
        for &member in members {
            let Some(deinflection) = deinflections.get_mut(member) else {
                continue;
            };
            if !filter || conditions_match(deinflection.conditions, definition_conditions) {
                deinflection.entries.push(bulk.clone());
            }
        }
    }
    Ok(truncated)
}

/// _getDictionaryEntries: one entry per row; a row reached again through a
/// longer transformed text replaces the earlier entry, through an equally
/// long one merges its rule chains (into the lists the entry shares with
/// its deinflection, see [`ChainTables`]).
fn dictionary_entries(
    deinflections: Vec<Deinflection>,
    dictionaries: &EnabledDictionaries,
    options: &FindTermsOptions,
    tables: &mut ChainTables,
    registry: &mut TagRegistry,
) -> (Vec<TermEntry>, usize) {
    let mut original_text_length = 0;
    // Each entry with the ids of the chain lists it shares.
    let mut entries: Vec<(TermEntry, usize, usize)> = Vec::new();
    let mut ids: HashSet<i64> = HashSet::new();
    for deinflection in deinflections {
        if deinflection.entries.is_empty() {
            continue;
        }
        original_text_length = original_text_length.max(utf16_len(&deinflection.original_text));
        let shared = (deinflection.text_processors, deinflection.inflections);
        for row in &deinflection.entries {
            let id = row.hit.id;
            if !ids.contains(&id) {
                let entry = create_entry(row, &deinflection, dictionaries, options, registry);
                entries.push((entry, shared.0, shared.1));
                ids.insert(id);
                continue;
            }
            let Some(existing_index) = entries
                .iter()
                .position(|(e, _, _)| e.definitions.iter().any(|d| d.id == id))
            else {
                continue;
            };
            let Some(existing) = entries.get_mut(existing_index) else {
                continue;
            };
            let existing_transformed = existing
                .0
                .headwords
                .first()
                .and_then(|h| h.sources.first())
                .map(|s| s.transformed_text.clone())
                .unwrap_or_default();
            let new_length = utf16_len(&deinflection.transformed_text);
            let existing_length = utf16_len(&existing_transformed);
            if new_length < existing_length {
                continue;
            }
            if new_length > existing_length {
                // Yomitan compares the new original text with the existing
                // transformed text here; kept as is for parity.
                if deinflection.original_text != existing_transformed {
                    *existing = (
                        create_entry(row, &deinflection, dictionaries, options, registry),
                        shared.0,
                        shared.1,
                    );
                }
            } else {
                let (target_processors, target_inflections) = (existing.1, existing.2);
                merge_inflection_chains(tables, target_inflections, shared.1);
                merge_text_processor_chains(tables, target_processors, shared.0);
            }
        }
    }
    let entries = entries
        .into_iter()
        .map(|(mut entry, processors, inflections)| {
            entry.text_processor_rule_chain_candidates =
                tables.text_processors(processors).to_vec();
            entry.inflection_rule_chain_candidates = tables.inflections(inflections).to_vec();
            entry
        })
        .collect();
    (entries, original_text_length)
}

fn create_entry(
    row: &BulkHit,
    deinflection: &Deinflection,
    dictionaries: &EnabledDictionaries,
    options: &FindTermsOptions,
    registry: &mut TagRegistry,
) -> TermEntry {
    let match_source = match row.match_source {
        MatchSource::Term => "term",
        MatchSource::Reading => "reading",
    };
    entry_from_hit(
        &row.hit,
        match_source,
        (
            &deinflection.original_text,
            &deinflection.transformed_text,
            &deinflection.deinflected_text,
        ),
        true,
        dictionaries,
        &options.primary_reading,
        registry,
    )
}

/// _createTermDictionaryEntryFromDatabaseEntry: a one-headword,
/// one-definition entry for a matched row, reached through `texts`
/// (original, transformed, deinflected). Rule chains are left empty for the
/// caller. Entries that are not primary (merge mode's related entries) keep
/// deinflection references in their glossary, as in Yomitan.
pub(crate) fn entry_from_hit(
    hit: &TermHit,
    match_source: &'static str,
    texts: (&str, &str, &str),
    is_primary: bool,
    dictionaries: &EnabledDictionaries,
    primary_reading: &str,
    registry: &mut TagRegistry,
) -> TermEntry {
    let (original_text, transformed_text, deinflected_text) = texts;
    let reading = if hit.reading.is_empty() {
        hit.term.clone()
    } else {
        hit.reading.clone()
    };
    let dictionary_index = dictionaries.index_of(hit.dictionary_id);
    let dictionary_alias = dictionaries.alias_of(hit.dictionary_id, &hit.dictionary_title);
    let match_primary_reading = !primary_reading.is_empty() && reading == primary_reading;
    let source_term_exact_match_count = u32::from(is_primary && deinflected_text == hit.term);
    let sequence = if hit.sequence >= 0 { hit.sequence } else { -1 };
    let source = Source {
        original_text: original_text.to_string(),
        transformed_text: transformed_text.to_string(),
        deinflected_text: deinflected_text.to_string(),
        match_type: "exact",
        match_source,
        is_primary,
    };
    let mut headword_tags = Vec::new();
    let mut definition_tags = Vec::new();
    registry.add(
        &mut headword_tags,
        hit.dictionary_id,
        &hit.dictionary_title,
        &hit.term_tags,
    );
    registry.add(
        &mut definition_tags,
        hit.dictionary_id,
        &hit.dictionary_title,
        &hit.definition_tags,
    );
    TermEntry {
        is_primary,
        // Filled from the shared chain lists once all merges are done.
        text_processor_rule_chain_candidates: Vec::new(),
        inflection_rule_chain_candidates: Vec::new(),
        score: hit.score,
        frequency_order: 0.0,
        dictionary_index,
        dictionary_alias: dictionary_alias.clone(),
        source_term_exact_match_count,
        match_primary_reading,
        max_original_text_length: utf16_len(original_text),
        headwords: vec![Headword {
            index: 0,
            headword_index: 0,
            term: hit.term.clone(),
            reading,
            sources: vec![source],
            tags: Vec::new(),
            tag_groups: headword_tags,
            word_classes: hit.rules.clone(),
        }],
        definitions: vec![Definition {
            index: 0,
            headword_indices: vec![0],
            dictionary: hit.dictionary_title.clone(),
            dictionary_id: hit.dictionary_id,
            dictionary_index,
            dictionary_alias,
            id: hit.id,
            score: hit.score,
            frequency_order: 0.0,
            sequences: vec![sequence],
            is_primary,
            tags: Vec::new(),
            tag_groups: definition_tags,
            entries: Vec::new(),
            glossary: None,
            raw_entries: !is_primary,
            has_form_of: !hit.form_of.is_empty(),
        }],
        pronunciations: Vec::new(),
        frequencies: Vec::new(),
    }
}

/// _mergeInflectionRuleChains into the shared list `target`.
fn merge_inflection_chains(tables: &mut ChainTables, target: usize, source: usize) {
    let candidates = tables.inflections(source).to_vec();
    let Some(existing) = tables.inflections.get_mut(target) else {
        return;
    };
    for candidate in candidates {
        let candidate_names = names(&candidate.inflection_rules);
        let duplicate = existing
            .iter_mut()
            .find(|chain| equal_ignore_order(&names(&chain.inflection_rules), &candidate_names));
        match duplicate {
            None => existing.push(candidate),
            Some(chain) => {
                if chain.source != candidate.source {
                    chain.source = InflectionSource::Both;
                }
            }
        }
    }
}

/// _mergeTextProcessorRuleChains into the shared list `target`.
fn merge_text_processor_chains(tables: &mut ChainTables, target: usize, source: usize) {
    let candidates = tables.text_processors(source).to_vec();
    let Some(existing) = tables.text_processors.get_mut(target) else {
        return;
    };
    for candidate in candidates {
        if !existing
            .iter()
            .any(|chain| equal_ignore_order(chain, &candidate))
        {
            existing.push(candidate);
        }
    }
}

fn names(rules: &[RuleName]) -> Vec<String> {
    rules.iter().map(|r| r.name.clone()).collect()
}

/// Same elements with the same multiplicities, in any order.
fn equal_ignore_order(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for item in a {
        *counts.entry(item.as_str()).or_insert(0) += 1;
    }
    for item in b {
        match counts.get_mut(item.as_str()) {
            Some(count) if *count > 0 => *count -= 1,
            _ => return false,
        }
    }
    true
}

/// _addUserFacingInflections: transform ids -> names and descriptions.
fn add_user_facing_inflections(entry: &mut TermEntry) {
    for chain in &mut entry.inflection_rule_chain_candidates {
        for rule in &mut chain.inflection_rules {
            let (name, description) = user_facing_rule(&rule.name);
            rule.name = name;
            rule.description = description.map(str::to_string);
        }
    }
}

/// _getJapaneseChineseKoreanOnlyText: the text up to the first character
/// that is neither Japanese, Chinese nor Korean.
pub(crate) fn japanese_chinese_korean_prefix(text: &str) -> &str {
    for (i, c) in text.char_indices() {
        let code_point = u32::from(c);
        if !is_code_point_japanese(code_point)
            && !is_code_point_chinese(code_point)
            && !is_code_point_korean(code_point)
        {
            return text.get(..i).unwrap_or_default();
        }
    }
    text
}
