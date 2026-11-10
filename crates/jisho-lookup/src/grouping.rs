// Ported from Yomitan ext/js/language/translator.js (_groupDictionaryEntriesByHeadword,
// _groupDictionaryEntriesByTerm, _groupDictionaryEntries, _getRelatedDictionaryEntries,
// _addRelatedDictionaryEntries, _addSecondaryRelatedDictionaryEntries,
// _createGroupedDictionaryEntry, _sortHeadwords, _getHeadwordMatchCounts,
// _addUniqueSimple, _addUniqueSources, _addTermHeadwords, _addUniqueTermHeadwordIndex,
// _addTermDefinitionsFast, _addTermDefinitions, _sortTermDictionaryEntriesById,
// _removeExcludedDefinitions, _removeUnusedHeadwords, _updateDefinitionHeadwordIndices,
// _updateArrayItemsHeadwordIndex, _removeArrayItemsWithDictionary)
// at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Result modes "group", "term" and "merge": combining one-definition
//! entries into entries with several headwords and definitions.

use std::collections::{HashMap, HashSet};

use jisho_store::{Store, StoreError};

use crate::entry::{Definition, Headword, InflectionSource, Source, TermEntry};
use crate::find_terms::entry_from_hit;
use crate::options::EnabledDictionaries;
use crate::tags::{TagRegistry, merge_groups};

/// What makes two definitions duplicates in merge mode: same dictionary
/// and same glossary (Yomitan compares `JSON.stringify([dictionary,
/// ...entries])`). The key is the id of the first definition with the same
/// content.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ContentKey(i64);

type GroupingKey = (String, Option<String>, Vec<(InflectionSource, Vec<String>)>);

/// _groupDictionaryEntriesByHeadword: same term, reading and inflection
/// chains.
pub(crate) fn group_by_headword(entries: Vec<TermEntry>, primary_reading: &str) -> Vec<TermEntry> {
    group(entries, primary_reading, true)
}

/// _groupDictionaryEntriesByTerm: same term and inflection chains.
pub(crate) fn group_by_term(entries: Vec<TermEntry>, primary_reading: &str) -> Vec<TermEntry> {
    group(entries, primary_reading, false)
}

fn group(entries: Vec<TermEntry>, primary_reading: &str, with_reading: bool) -> Vec<TermEntry> {
    let mut groups: Vec<Vec<TermEntry>> = Vec::new();
    let mut positions: HashMap<GroupingKey, usize> = HashMap::new();
    for entry in entries {
        let key = grouping_key(&entry, with_reading);
        match positions.get(&key).and_then(|&i| groups.get_mut(i)) {
            Some(group) => group.push(entry),
            None => {
                positions.insert(key, groups.len());
                groups.push(vec![entry]);
            }
        }
    }
    groups
        .iter()
        .map(|g| grouped_entry(g, None, primary_reading))
        .collect()
}

fn grouping_key(entry: &TermEntry, with_reading: bool) -> GroupingKey {
    let (term, reading) = entry
        .headwords
        .first()
        .map(|h| (h.term.clone(), h.reading.clone()))
        .unwrap_or_default();
    let chains = entry
        .inflection_rule_chain_candidates
        .iter()
        .map(|c| {
            (
                c.source,
                c.inflection_rules.iter().map(|r| r.name.clone()).collect(),
            )
        })
        .collect();
    (term, with_reading.then_some(reading), chains)
}

/// A merge-mode group: entries sharing a main-dictionary sequence number.
struct SequenceGroup {
    ids: HashSet<i64>,
    entries: Vec<TermEntry>,
}

/// Entries by id in insertion order, with removal (a JavaScript Map).
#[derive(Default)]
struct OrderedEntries {
    slots: Vec<Option<(i64, TermEntry)>>,
    positions: HashMap<i64, usize>,
}

impl OrderedEntries {
    fn set(&mut self, id: i64, entry: TermEntry) {
        match self.positions.get(&id).and_then(|&i| self.slots.get_mut(i)) {
            Some(slot) => *slot = Some((id, entry)),
            None => {
                self.positions.insert(id, self.slots.len());
                self.slots.push(Some((id, entry)));
            }
        }
    }

    fn delete(&mut self, id: i64) {
        if let Some(slot) = self
            .positions
            .remove(&id)
            .and_then(|i| self.slots.get_mut(i))
        {
            *slot = None;
        }
    }

    fn len(&self) -> usize {
        self.positions.len()
    }

    fn ids(&self) -> Vec<i64> {
        self.slots.iter().flatten().map(|(id, _)| *id).collect()
    }

    fn get(&self, id: i64) -> Option<&TermEntry> {
        self.positions
            .get(&id)
            .and_then(|&i| self.slots.get(i))
            .and_then(Option::as_ref)
            .map(|(_, e)| e)
    }

    fn into_values(self) -> Vec<TermEntry> {
        self.slots.into_iter().flatten().map(|(_, e)| e).collect()
    }
}

/// _getRelatedDictionaryEntries (merge mode): entries of the main
/// dictionary grouped by sequence, with the other entries of each sequence
/// and, by headword, other dictionaries' entries; the rest grouped by
/// headword.
pub(crate) fn merge(
    store: &Store,
    entries: Vec<TermEntry>,
    dictionaries: &EnabledDictionaries,
    primary_reading: &str,
    registry: &mut TagRegistry,
) -> Result<Vec<TermEntry>, StoreError> {
    let main = dictionaries.main_dictionary.as_ref().map(|(id, _)| *id);
    let mut sequence_list: Vec<(i64, i64)> = Vec::new();
    let mut groups: Vec<SequenceGroup> = Vec::new();
    let mut by_sequence: HashMap<i64, usize> = HashMap::new();
    let mut ungrouped = OrderedEntries::default();
    for entry in entries {
        let Some(definition) = entry.definitions.first() else {
            continue;
        };
        let (id, dictionary_id) = (definition.id, definition.dictionary_id);
        let sequence = definition.sequences.first().copied().unwrap_or(-1);
        if main == Some(dictionary_id) && sequence >= 0 {
            let index = *by_sequence.entry(sequence).or_insert_with(|| {
                sequence_list.push((sequence, dictionary_id));
                groups.push(SequenceGroup {
                    ids: HashSet::new(),
                    entries: Vec::new(),
                });
                groups.len() - 1
            });
            if let Some(group) = groups.get_mut(index) {
                group.entries.push(entry);
                group.ids.insert(id);
            }
        } else {
            ungrouped.set(id, entry);
        }
    }

    if !sequence_list.is_empty() {
        let secondary: Vec<i64> = dictionaries
            .ids()
            .into_iter()
            .filter(|id| {
                dictionaries
                    .by_id
                    .get(id)
                    .is_some_and(|d| d.allow_secondary_searches)
            })
            .collect();
        add_related(
            store,
            &mut groups,
            &mut ungrouped,
            &sequence_list,
            dictionaries,
            primary_reading,
            registry,
        )?;
        for group in &mut groups {
            group
                .entries
                .sort_by_key(|e| e.definitions.first().map_or(0, |d| d.id));
        }
        if ungrouped.len() > 0 || !secondary.is_empty() {
            add_secondary(
                store,
                &mut groups,
                &mut ungrouped,
                dictionaries,
                &secondary,
                primary_reading,
                registry,
            )?;
        }
    }

    let mut result = Vec::with_capacity(groups.len());
    for group in &groups {
        let keys = content_keys(store, &group.entries)?;
        result.push(grouped_entry(&group.entries, Some(&keys), primary_reading));
    }
    result.extend(group_by_headword(ungrouped.into_values(), primary_reading));
    Ok(result)
}

/// _addRelatedDictionaryEntries: the other rows of each group's sequence.
fn add_related(
    store: &Store,
    groups: &mut [SequenceGroup],
    ungrouped: &mut OrderedEntries,
    sequence_list: &[(i64, i64)],
    dictionaries: &EnabledDictionaries,
    primary_reading: &str,
    registry: &mut TagRegistry,
) -> Result<(), StoreError> {
    for (index, hit) in store.find_terms_by_sequence(sequence_list)? {
        let Some(group) = groups.get_mut(index) else {
            continue;
        };
        if group.ids.contains(&hit.id) {
            continue;
        }
        let term = hit.term.clone();
        let entry = entry_from_hit(
            &hit,
            "sequence",
            (&term, &term, &term),
            false,
            dictionaries,
            primary_reading,
            registry,
        );
        group.entries.push(entry);
        group.ids.insert(hit.id);
        ungrouped.delete(hit.id);
    }
    Ok(())
}

/// _addSecondaryRelatedDictionaryEntries: ungrouped entries and, from
/// dictionaries allowing secondary searches, rows with the term and
/// reading of a grouped entry join that group.
fn add_secondary(
    store: &Store,
    groups: &mut [SequenceGroup],
    ungrouped: &mut OrderedEntries,
    dictionaries: &EnabledDictionaries,
    secondary: &[i64],
    primary_reading: &str,
    registry: &mut TagRegistry,
) -> Result<(), StoreError> {
    let mut term_list: Vec<(String, String)> = Vec::new();
    let mut targets: Vec<Vec<usize>> = Vec::new();
    let mut target_positions: HashMap<(String, String), usize> = HashMap::new();
    for (group_index, group) in groups.iter().enumerate() {
        for entry in &group.entries {
            let Some(headword) = entry.headwords.first() else {
                continue;
            };
            let key = (headword.term.clone(), headword.reading.clone());
            let position = *target_positions.entry(key.clone()).or_insert_with(|| {
                term_list.push(key);
                targets.push(Vec::new());
                targets.len() - 1
            });
            if let Some(target) = targets.get_mut(position) {
                target.push(group_index);
            }
        }
    }

    for id in ungrouped.ids() {
        let Some(entry) = ungrouped.get(id) else {
            continue;
        };
        let Some(headword) = entry.headwords.first() else {
            continue;
        };
        let key = (headword.term.clone(), headword.reading.clone());
        let Some(target) = target_positions.get(&key).and_then(|&i| targets.get(i)) else {
            continue;
        };
        let entry = entry.clone();
        for &group_index in target {
            if let Some(group) = groups.get_mut(group_index)
                && !group.ids.contains(&id)
            {
                group.entries.push(entry.clone());
                group.ids.insert(id);
            }
        }
        ungrouped.delete(id);
    }

    if term_list.is_empty() || secondary.is_empty() {
        return Ok(());
    }
    for (index, hit) in store.find_terms_exact(&term_list, secondary)? {
        let (Some((source_text, _)), Some(target)) = (term_list.get(index), targets.get(index))
        else {
            continue;
        };
        for &group_index in target {
            let Some(group) = groups.get_mut(group_index) else {
                continue;
            };
            if group.ids.contains(&hit.id) {
                continue;
            }
            let entry = entry_from_hit(
                &hit,
                "term",
                (source_text, source_text, source_text),
                false,
                dictionaries,
                primary_reading,
                registry,
            );
            group.entries.push(entry);
            group.ids.insert(hit.id);
            ungrouped.delete(hit.id);
        }
    }
    Ok(())
}

/// Content keys of the definitions of a merge group, for duplicate
/// detection. Only definitions that can have a twin are compared: in a
/// dictionary with a definition whose deinflection references are dropped
/// (its content is shorter than the stored row), all of them; otherwise
/// those whose stored size equals another's. Their contents are hashed one
/// row at a time and equal hashes confirmed, so memory stays at a couple of
/// glossaries whatever the group size.
fn content_keys(
    store: &Store,
    entries: &[TermEntry],
) -> Result<HashMap<i64, ContentKey>, StoreError> {
    let definitions: Vec<&Definition> = entries.iter().flat_map(|e| &e.definitions).collect();
    let mut keys: HashMap<i64, ContentKey> = definitions
        .iter()
        .map(|d| (d.id, ContentKey(d.id)))
        .collect();
    if definitions.len() < 2 {
        return Ok(keys);
    }
    let ids: Vec<i64> = definitions.iter().map(|d| d.id).collect();
    let sizes = store.glossary_sizes(&ids)?;
    let mut by_dictionary: Vec<(i64, Vec<&Definition>)> = Vec::new();
    for definition in &definitions {
        match by_dictionary
            .iter_mut()
            .find(|(id, _)| *id == definition.dictionary_id)
        {
            Some((_, list)) => list.push(definition),
            None => by_dictionary.push((definition.dictionary_id, vec![definition])),
        }
    }
    let mut candidates: Vec<(i64, bool)> = Vec::new();
    for (_, list) in &by_dictionary {
        if list.len() < 2 {
            continue;
        }
        let filtered = list.iter().any(|d| !d.raw_entries && d.has_form_of);
        let size = |d: &Definition| sizes.get(&d.id).copied().unwrap_or(0);
        for definition in list {
            let twin_sized = list
                .iter()
                .any(|other| other.id != definition.id && size(other) == size(definition));
            if filtered || twin_sized {
                candidates.push((definition.id, definition.raw_entries));
            }
        }
    }
    if candidates.is_empty() {
        return Ok(keys);
    }
    let hashes = store.glossary_content_hashes(&candidates)?;
    // First definition of each (dictionary, hash), in group order.
    let mut firsts: HashMap<(i64, u64), Vec<(i64, bool)>> = HashMap::new();
    let keep: HashMap<i64, bool> = candidates.iter().copied().collect();
    for definition in &definitions {
        let (Some(&hash), Some(&keep_references)) =
            (hashes.get(&definition.id), keep.get(&definition.id))
        else {
            continue;
        };
        let representatives = firsts.entry((definition.dictionary_id, hash)).or_default();
        let mut twin = None;
        if !representatives.is_empty() {
            let content = store.glossary_content(definition.id, keep_references)?;
            for &(id, keep_other) in representatives.iter() {
                if store.glossary_content(id, keep_other)? == content {
                    twin = Some(id);
                    break;
                }
            }
        }
        match twin {
            Some(id) => {
                keys.insert(definition.id, ContentKey(id));
            }
            None => representatives.push((definition.id, keep_references)),
        }
    }
    Ok(keys)
}

/// _createGroupedDictionaryEntry. With `content_keys` (merge mode, more
/// than one entry), definitions with the same dictionary and glossary are
/// merged.
fn grouped_entry(
    entries: &[TermEntry],
    content_keys: Option<&HashMap<i64, ContentKey>>,
    primary_reading: &str,
) -> TermEntry {
    let mut headwords: Vec<Headword> = Vec::new();
    let mut headword_positions: HashMap<(String, String), usize> = HashMap::new();
    let mut headword_dictionary_indices: HashMap<usize, usize> = HashMap::new();
    let mut headword_maps: Vec<Vec<usize>> = Vec::with_capacity(entries.len());
    for entry in entries {
        let map = add_headwords(&mut headwords, &mut headword_positions, &entry.headwords);
        for &index in &map {
            let existing = headword_dictionary_indices
                .entry(index)
                .or_insert(usize::MAX);
            *existing = (*existing).min(entry.dictionary_index);
        }
        headword_maps.push(map);
    }

    let content_keys = if entries.len() <= 1 {
        None
    } else {
        content_keys
    };
    let mut score = -9_007_199_254_740_991.0_f64;
    let mut dictionary_index = usize::MAX;
    let mut max_original_text_length = 0;
    let mut is_primary = false;
    let mut definitions: Vec<Definition> = Vec::new();
    let mut definition_positions: HashMap<(i64, ContentKey), usize> = HashMap::new();
    let mut inflections = None;
    let mut text_processes = None;
    for (entry, map) in entries.iter().zip(&headword_maps) {
        score = score.max(entry.score);
        dictionary_index = dictionary_index.min(entry.dictionary_index);
        if entry.is_primary {
            is_primary = true;
            max_original_text_length = max_original_text_length.max(entry.max_original_text_length);
            let chains = &entry.inflection_rule_chain_candidates;
            if inflections.is_none_or(|current: &Vec<_>| chains.len() < current.len()) {
                inflections = Some(chains);
            }
            let processes = &entry.text_processor_rule_chain_candidates;
            if text_processes.is_none_or(|current: &Vec<_>| processes.len() < current.len()) {
                text_processes = Some(processes);
            }
        }
        match content_keys {
            Some(keys) => add_definitions(
                &mut definitions,
                &mut definition_positions,
                &entry.definitions,
                map,
                keys,
            ),
            None => add_definitions_fast(&mut definitions, &entry.definitions, map),
        }
    }

    sort_headwords(
        &mut headwords,
        &headword_dictionary_indices,
        &mut definitions,
    );
    let (source_term_exact_match_count, match_primary_reading) =
        headword_match_counts(&headwords, primary_reading);
    TermEntry {
        is_primary,
        text_processor_rule_chain_candidates: text_processes.cloned().unwrap_or_default(),
        inflection_rule_chain_candidates: inflections.cloned().unwrap_or_default(),
        score,
        frequency_order: 0.0,
        dictionary_index,
        dictionary_alias: String::new(),
        source_term_exact_match_count,
        match_primary_reading,
        max_original_text_length,
        headwords,
        definitions,
        pronunciations: Vec::new(),
        frequencies: Vec::new(),
    }
}

/// _addTermHeadwords: merges `new` into `headwords` by (term, reading);
/// returns the merged index of each.
fn add_headwords(
    headwords: &mut Vec<Headword>,
    positions: &mut HashMap<(String, String), usize>,
    new: &[Headword],
) -> Vec<usize> {
    let mut map = Vec::with_capacity(new.len());
    for headword in new {
        let key = (headword.term.clone(), headword.reading.clone());
        let index = *positions.entry(key).or_insert_with(|| {
            let index = headwords.len();
            headwords.push(Headword {
                index,
                headword_index: index,
                term: headword.term.clone(),
                reading: headword.reading.clone(),
                sources: Vec::new(),
                tags: Vec::new(),
                tag_groups: Vec::new(),
                word_classes: Vec::new(),
            });
            index
        });
        if let Some(target) = headwords.get_mut(index) {
            add_unique_sources(&mut target.sources, &headword.sources);
            for word_class in &headword.word_classes {
                if !target.word_classes.contains(word_class) {
                    target.word_classes.push(word_class.clone());
                }
            }
            merge_groups(&mut target.tag_groups, &headword.tag_groups);
        }
        map.push(index);
    }
    map
}

/// _addUniqueSources: a source already present (all fields but
/// `is_primary` equal) only becomes primary if the new one is.
fn add_unique_sources(sources: &mut Vec<Source>, new: &[Source]) {
    if new.is_empty() {
        return;
    }
    if sources.is_empty() {
        sources.extend(new.iter().cloned());
        return;
    }
    for source in new {
        let existing = sources.iter_mut().find(|s| {
            s.deinflected_text == source.deinflected_text
                && s.transformed_text == source.transformed_text
                && s.original_text == source.original_text
                && s.match_type == source.match_type
                && s.match_source == source.match_source
        });
        match existing {
            Some(existing) => {
                if source.is_primary {
                    existing.is_primary = true;
                }
            }
            None => sources.push(source.clone()),
        }
    }
}

/// _addTermDefinitionsFast: definitions copied, headword indices mapped.
fn add_definitions_fast(definitions: &mut Vec<Definition>, new: &[Definition], map: &[usize]) {
    for definition in new {
        let mut copy = definition.clone();
        copy.index = definitions.len();
        copy.headword_indices = definition
            .headword_indices
            .iter()
            .map(|&i| map.get(i).copied().unwrap_or(i))
            .collect();
        copy.frequency_order = 0.0;
        definitions.push(copy);
    }
}

/// _addTermDefinitions: definitions with the same dictionary and content
/// merged (sequences, headword indices, tags, primary flag).
fn add_definitions(
    definitions: &mut Vec<Definition>,
    positions: &mut HashMap<(i64, ContentKey), usize>,
    new: &[Definition],
    map: &[usize],
    keys: &HashMap<i64, ContentKey>,
) {
    for definition in new {
        let content = keys
            .get(&definition.id)
            .cloned()
            .unwrap_or(ContentKey(definition.id));
        let key = (definition.dictionary_id, content);
        let position = match positions.get(&key) {
            Some(&position) => {
                if let Some(existing) = definitions.get_mut(position) {
                    if definition.is_primary {
                        existing.is_primary = true;
                    }
                    for sequence in &definition.sequences {
                        if !existing.sequences.contains(sequence) {
                            existing.sequences.push(*sequence);
                        }
                    }
                }
                position
            }
            None => {
                let position = definitions.len();
                let mut copy = definition.clone();
                copy.index = position;
                copy.headword_indices = Vec::new();
                copy.frequency_order = 0.0;
                copy.tag_groups = Vec::new();
                definitions.push(copy);
                positions.insert(key, position);
                position
            }
        };
        if let Some(target) = definitions.get_mut(position) {
            for &index in &definition.headword_indices {
                add_unique_headword_index(
                    &mut target.headword_indices,
                    map.get(index).copied().unwrap_or(index),
                );
            }
            merge_groups(&mut target.tag_groups, &definition.tag_groups);
        }
    }
}

/// _addUniqueTermHeadwordIndex: inserts into a sorted list unless present.
fn add_unique_headword_index(indices: &mut Vec<usize>, index: usize) {
    if let Err(position) = indices.binary_search(&index) {
        indices.insert(position, index);
    }
}

/// _sortHeadwords: headwords with a primary source first, then by the
/// smallest dictionary index of their entries; indices renumbered.
fn sort_headwords(
    headwords: &mut [Headword],
    dictionary_indices: &HashMap<usize, usize>,
    definitions: &mut [Definition],
) {
    let rank = |h: &Headword| {
        (
            !h.sources.iter().any(|s| s.is_primary),
            dictionary_indices
                .get(&h.index)
                .copied()
                .unwrap_or(usize::MAX),
        )
    };
    headwords.sort_by_key(rank);
    let mut remap: HashMap<usize, usize> = HashMap::new();
    for (i, headword) in headwords.iter_mut().enumerate() {
        remap.insert(headword.index, i);
        headword.index = i;
        headword.headword_index = i;
    }
    for definition in definitions {
        for index in &mut definition.headword_indices {
            if let Some(&new) = remap.get(index) {
                *index = new;
            }
        }
    }
}

/// _getHeadwordMatchCounts.
fn headword_match_counts(headwords: &[Headword], primary_reading: &str) -> (u32, bool) {
    let mut count = 0;
    let mut match_primary_reading = false;
    for headword in headwords {
        if !primary_reading.is_empty() && headword.reading == primary_reading {
            match_primary_reading = true;
        }
        if headword
            .sources
            .iter()
            .any(|s| s.is_primary && s.match_source == "term")
        {
            count += 1;
        }
    }
    (count, match_primary_reading)
}

/// _removeExcludedDefinitions: drops the definitions (and metadata) of
/// dictionary `excluded`; entries left without definitions are dropped,
/// headwords no definition uses are removed. Tag groups are not touched:
/// Yomitan filters the (still empty) expanded tag lists here, which has no
/// effect.
pub(crate) fn remove_excluded_definitions(entries: &mut Vec<TermEntry>, excluded: i64) {
    entries.retain_mut(|entry| {
        let before = entry.definitions.len();
        entry.definitions.retain(|d| d.dictionary_id != excluded);
        entry.pronunciations.retain(|p| p.dictionary_id != excluded);
        entry.frequencies.retain(|f| f.dictionary_id != excluded);
        if entry.definitions.len() == before {
            return true;
        }
        if entry.definitions.is_empty() {
            return false;
        }
        remove_unused_headwords(entry);
        true
    });
}

/// _removeUnusedHeadwords.
fn remove_unused_headwords(entry: &mut TermEntry) {
    let used: HashSet<usize> = entry
        .definitions
        .iter()
        .flat_map(|d| d.headword_indices.iter().copied())
        .collect();
    if (0..entry.headwords.len()).all(|i| used.contains(&i)) {
        return;
    }
    let mut remap: HashMap<usize, usize> = HashMap::new();
    let mut kept = Vec::with_capacity(entry.headwords.len());
    for (old, mut headword) in std::mem::take(&mut entry.headwords).into_iter().enumerate() {
        if used.contains(&old) {
            let new = remap.len();
            remap.insert(old, new);
            headword.headword_index = new;
            kept.push(headword);
        }
    }
    entry.headwords = kept;
    for definition in &mut entry.definitions {
        definition.headword_indices = definition
            .headword_indices
            .iter()
            .filter_map(|i| remap.get(i).copied())
            .collect();
    }
    entry
        .pronunciations
        .retain_mut(|p| match remap.get(&p.headword_index) {
            Some(&new) => {
                p.headword_index = new;
                true
            }
            None => false,
        });
    entry
        .frequencies
        .retain_mut(|f| match remap.get(&f.headword_index) {
            Some(&new) => {
                f.headword_index = new;
                true
            }
            None => false,
        });
}
