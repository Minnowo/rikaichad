// Ported from Yomitan ext/js/language/translator.js (findTerms for the
// "split" mode, _findTermsInternal, _getDeinflections,
// _getDictionaryDeinflections, _addEntriesToDeinflections,
// _groupDeinflectionsByTerm, _matchEntriesToDeinflections,
// _getDictionaryEntries, _mergeInflectionRuleChains,
// _mergeTextProcessorRuleChains, _areArraysEqualIgnoreOrder,
// _getJapaneseChineseKoreanOnlyText,
// _createTermDictionaryEntryFromDatabaseEntry, _addUserFacingInflections)
// at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Term lookup: candidates -> matched dictionary rows -> entries, sorted.

use std::collections::{HashMap, HashSet};

use jisho_ja::{
    condition_flags_from_parts_of_speech, conditions_match, is_code_point_chinese,
    is_code_point_japanese, is_code_point_korean, user_facing_rule,
};
use jisho_store::{BulkHit, MatchSource, Store, StoreError};

use crate::deinflection::{
    ChainTables, Deinflection, algorithm_deinflections, identity_deinflection,
};
use crate::entry::{
    Definition, FindTermsResult, Headword, InflectionChain, InflectionSource, RuleName, Source,
    TermEntry,
};
use crate::options::{EnabledDictionaries, FindTermsOptions};
use crate::sort::sort_term_entries;
use crate::utf16::utf16_len;

/// Finds the dictionary entries for the start of `text`, like Yomitan's
/// findTerms in "split" mode (one headword and one definition per entry),
/// sorted. Glossaries are not loaded; see [`load_glossaries`].
pub fn find_terms(
    store: &Store,
    text: &str,
    dictionaries: &EnabledDictionaries,
    options: &FindTermsOptions,
) -> Result<FindTermsResult, StoreError> {
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
    let mut tables = ChainTables::default();
    let mut truncated = false;
    let deinflections = deinflections(
        store,
        text,
        dictionaries,
        options,
        &mut tables,
        &mut truncated,
    )?;
    let (mut entries, original_text_length) =
        dictionary_entries(deinflections, dictionaries, options, &mut tables);
    if entries.len() > 1 {
        sort_term_entries(&mut entries);
    }
    for entry in &mut entries {
        add_user_facing_inflections(entry);
    }
    Ok(FindTermsResult {
        entries,
        original_text_length,
        truncated,
    })
}

/// Loads the glossaries of the definitions in `entries`, dropping
/// deinflection-reference items as Yomitan does, until `max_bytes` of
/// stored glossary JSON is used. Returns how many leading entries were
/// filled completely; later entries keep empty glossaries and should be
/// dropped by the caller.
pub fn load_glossaries(
    store: &Store,
    entries: &mut [TermEntry],
    max_bytes: usize,
) -> Result<usize, StoreError> {
    let ids: Vec<i64> = entries
        .iter()
        .flat_map(|e| e.definitions.iter().map(|d| d.id))
        .collect();
    let (mut glossaries, _) = store.glossaries_within(&ids, max_bytes)?;
    let mut filled = 0;
    for entry in entries.iter_mut() {
        let mut complete = true;
        for definition in &mut entry.definitions {
            match glossaries.remove(&definition.id) {
                Some(items) => {
                    definition.entries =
                        items.into_iter().filter(|item| !item.is_array()).collect();
                }
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
    let (rows, truncated) = store.find_terms_bulk(&terms, options.max_hits)?;
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
                let entry = create_entry(row, &deinflection, dictionaries, options);
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
                        create_entry(row, &deinflection, dictionaries, options),
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
) -> TermEntry {
    let hit = &row.hit;
    let reading = if hit.reading.is_empty() {
        hit.term.clone()
    } else {
        hit.reading.clone()
    };
    let dictionary = dictionaries.by_id.get(&hit.dictionary_id);
    let dictionary_index = dictionary.map_or(0, |d| d.index);
    let dictionary_alias =
        dictionary.map_or_else(|| hit.dictionary_title.clone(), |d| d.alias.clone());
    let match_primary_reading =
        !options.primary_reading.is_empty() && reading == options.primary_reading;
    let source_term_exact_match_count = u32::from(deinflection.deinflected_text == hit.term);
    let sequence = if hit.sequence >= 0 { hit.sequence } else { -1 };
    let source = Source {
        original_text: deinflection.original_text.clone(),
        transformed_text: deinflection.transformed_text.clone(),
        deinflected_text: deinflection.deinflected_text.clone(),
        match_type: "exact",
        match_source: match row.match_source {
            MatchSource::Term => "term",
            MatchSource::Reading => "reading",
        },
        is_primary: true,
    };
    TermEntry {
        is_primary: true,
        // Filled from the shared chain lists once all merges are done.
        text_processor_rule_chain_candidates: Vec::new(),
        inflection_rule_chain_candidates: Vec::new(),
        score: hit.score,
        frequency_order: 0,
        dictionary_index,
        dictionary_alias: dictionary_alias.clone(),
        source_term_exact_match_count,
        match_primary_reading,
        max_original_text_length: utf16_len(&deinflection.original_text),
        headwords: vec![Headword {
            index: 0,
            headword_index: 0,
            term: hit.term.clone(),
            reading,
            sources: vec![source],
            tags: hit.term_tags.clone(),
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
            frequency_order: 0,
            sequences: vec![sequence],
            is_primary: true,
            tags: hit.definition_tags.clone(),
            entries: Vec::new(),
        }],
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
fn japanese_chinese_korean_prefix(text: &str) -> &str {
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
