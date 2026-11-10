// Ported from Yomitan ext/js/language/translator.js (findKanji,
// _addKanjiMeta, _expandKanjiStats, _sortKanjiStats, _createKanjiStat,
// _createKanjiFrequency, _createKanjiDictionaryEntry,
// _sortKanjiDictionaryEntries, _sortKanjiDictionaryEntryData) at
// ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Kanji lookup: every distinct character of the text in the kanji
//! dictionaries, with stats, tags and frequencies.

use jisho_store::{KanjiHit, Store, StoreError};

use crate::collation;
use crate::entry::{KanjiEntry, KanjiFrequency, KanjiStat, TagGroup};
use crate::find_terms::japanese_chinese_korean_prefix;
use crate::meta::frequency_info;
use crate::options::EnabledDictionaries;
use crate::tags::{TagLookup, TagRegistry};

/// Finds the kanji of `text` (each distinct character, in order) in the
/// enabled dictionaries, like Yomitan's findKanji: sorted by dictionary
/// order, then character order. `remove_non_japanese_characters` cuts the
/// text first (Yomitan's default: false).
pub fn find_kanji(
    store: &Store,
    text: &str,
    dictionaries: &EnabledDictionaries,
    remove_non_japanese_characters: bool,
) -> Result<Vec<KanjiEntry>, StoreError> {
    let text = if remove_non_japanese_characters {
        japanese_chinese_korean_prefix(text)
    } else {
        text
    };
    let mut characters: Vec<String> = Vec::new();
    for c in text.chars() {
        let c = c.to_string();
        if !characters.contains(&c) {
            characters.push(c);
        }
    }
    let ids = dictionaries.ids();
    let hits = store.find_kanji(&characters, &ids)?;
    if hits.is_empty() {
        return Ok(Vec::new());
    }

    let mut registry = TagRegistry::default();
    let stat_groups: Vec<TagGroup> = hits
        .iter()
        .map(|hit| TagGroup {
            dictionary_id: hit.dictionary_id,
            dictionary: hit.dictionary_title.clone(),
            names: hit.stats.iter().map(|(name, _)| name.clone()).collect(),
        })
        .collect();
    let stat_tags = TagLookup::load(store, &stat_groups)?;
    let mut entries: Vec<KanjiEntry> = hits
        .into_iter()
        .map(|hit| create_entry(hit, dictionaries, &stat_tags, &mut registry))
        .collect();
    entries.sort_by_key(|e| e.dictionary_index);

    add_kanji_meta(store, &mut entries, dictionaries, &ids)?;
    let lookup = TagLookup::load(store, entries.iter().flat_map(|e| &e.tag_groups))?;
    for entry in &mut entries {
        entry.tags = lookup.expand(&registry, &entry.tag_groups);
        entry
            .frequencies
            .sort_by_key(|f| (f.dictionary_index, f.index));
    }
    Ok(entries)
}

fn create_entry(
    hit: KanjiHit,
    dictionaries: &EnabledDictionaries,
    stat_tags: &TagLookup,
    registry: &mut TagRegistry,
) -> KanjiEntry {
    let stats = expand_stats(&hit, stat_tags);
    let mut tag_groups = Vec::new();
    registry.add(
        &mut tag_groups,
        hit.dictionary_id,
        &hit.dictionary_title,
        &hit.tags,
    );
    KanjiEntry {
        dictionary_index: dictionaries.index_of(hit.dictionary_id),
        dictionary_alias: dictionaries.alias_of(hit.dictionary_id, &hit.dictionary_title),
        character: hit.character,
        dictionary: hit.dictionary_title,
        dictionary_id: hit.dictionary_id,
        onyomi: hit.onyomi,
        kunyomi: hit.kunyomi,
        tags: Vec::new(),
        tag_groups,
        stats,
        definitions: hit.meanings,
        frequencies: Vec::new(),
    }
}

/// _expandKanjiStats: stats with a tag of their name, grouped by the tag's
/// category (first-seen order), each group sorted by order then notes.
fn expand_stats(hit: &KanjiHit, tags: &TagLookup) -> Vec<(String, Vec<KanjiStat>)> {
    let mut groups: Vec<(String, Vec<KanjiStat>)> = Vec::new();
    for (name, value) in &hit.stats {
        let Some(tag) = tags.record(hit.dictionary_id, name) else {
            continue;
        };
        let stat = KanjiStat {
            name: name.clone(),
            category: if tag.category.is_empty() {
                "default".to_string()
            } else {
                tag.category.clone()
            },
            content: tag.notes.clone(),
            order: tag.order,
            score: tag.score,
            dictionary: hit.dictionary_title.clone(),
            value: value.clone(),
        };
        // Grouped by the raw category, as Yomitan does.
        match groups.iter_mut().find(|(c, _)| *c == tag.category) {
            Some((_, group)) => group.push(stat),
            None => groups.push((tag.category.clone(), vec![stat])),
        }
    }
    for (_, group) in &mut groups {
        if group.len() > 1 {
            group.sort_by(|a, b| {
                a.order
                    .partial_cmp(&b.order)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| collation::compare(&a.content, &b.content))
            });
        }
    }
    groups
}

/// _addKanjiMeta: frequencies of each entry's character.
fn add_kanji_meta(
    store: &Store,
    entries: &mut [KanjiEntry],
    dictionaries: &EnabledDictionaries,
    ids: &[i64],
) -> Result<(), StoreError> {
    let mut characters: Vec<String> = Vec::new();
    for entry in entries.iter() {
        if !characters.contains(&entry.character) {
            characters.push(entry.character.clone());
        }
    }
    let metas = store.find_kanji_meta(&characters, ids)?;
    for entry in entries.iter_mut() {
        for meta in metas.iter().filter(|m| m.character == entry.character) {
            if meta.mode != "freq" {
                continue;
            }
            let info = frequency_info(&meta.data);
            let title = dictionaries.title_of(meta.dictionary_id);
            entry.frequencies.push(KanjiFrequency {
                index: entry.frequencies.len(),
                dictionary_index: dictionaries.index_of(meta.dictionary_id),
                dictionary_alias: dictionaries.alias_of(meta.dictionary_id, &title),
                dictionary: title,
                dictionary_id: meta.dictionary_id,
                character: meta.character.clone(),
                frequency: info.frequency,
                display_value: info.display_value,
                display_value_parsed: info.display_value_parsed,
                frequency_mode: dictionaries
                    .frequency_modes
                    .get(&meta.dictionary_id)
                    .cloned(),
            });
        }
    }
    Ok(())
}
