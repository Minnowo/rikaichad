// Ported from Yomitan ext/js/language/translator.js (TranslatorTagAggregator,
// _expandTagGroups, _groupTags, _mergeSimilarTags, _getTagNamesWithCategory,
// _flagRedundantDefinitionTags, _createTag, _getNameBase) at
// ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Tags: entries carry tag names grouped by dictionary while they are built
//! and merged; at the end the names are looked up in the dictionaries' tag
//! banks and become [`Tag`]s.
//!
//! Yomitan expands all tag targets in one pass that walks the (dictionary,
//! name) pairs in the order they were first added anywhere, so every
//! target receives its tags in that global order (visible in a merged
//! tag's `dictionaries`). [`TagRegistry`] records that order.

use std::collections::HashMap;

use jisho_store::{Store, StoreError, TagRecord};

use crate::collation;
use crate::entry::{Definition, Tag, TagGroup};

/// First-added order of (dictionary id, tag name) pairs.
#[derive(Debug, Default)]
pub(crate) struct TagRegistry {
    rank: HashMap<(i64, String), usize>,
}

impl TagRegistry {
    /// TranslatorTagAggregator.addTags: adds `names` to the group of
    /// `dictionary` in `groups`.
    pub fn add(
        &mut self,
        groups: &mut Vec<TagGroup>,
        dictionary_id: i64,
        dictionary: &str,
        names: &[String],
    ) {
        if names.is_empty() {
            return;
        }
        for name in names {
            let next = self.rank.len();
            self.rank
                .entry((dictionary_id, name.clone()))
                .or_insert(next);
        }
        add_to_group(groups, dictionary_id, dictionary, names);
    }

    fn rank_of(&self, dictionary_id: i64, name: &str) -> usize {
        self.rank
            .get(&(dictionary_id, name.to_string()))
            .copied()
            .unwrap_or(usize::MAX)
    }
}

/// TranslatorTagAggregator.mergeTags: adds the groups `new` to `groups`.
pub(crate) fn merge_groups(groups: &mut Vec<TagGroup>, new: &[TagGroup]) {
    for group in new {
        add_to_group(groups, group.dictionary_id, &group.dictionary, &group.names);
    }
}

/// Adds `names` to the group of `dictionary_id` in `groups`, creating the
/// group at the end if missing.
fn add_to_group(
    groups: &mut Vec<TagGroup>,
    dictionary_id: i64,
    dictionary: &str,
    names: &[String],
) {
    if let Some(group) = groups.iter_mut().find(|g| g.dictionary_id == dictionary_id) {
        add_unique_names(group, names);
        return;
    }
    let mut group = TagGroup {
        dictionary_id,
        dictionary: dictionary.to_string(),
        names: Vec::new(),
    };
    add_unique_names(&mut group, names);
    groups.push(group);
}

fn add_unique_names(group: &mut TagGroup, names: &[String]) {
    for name in names {
        if !group.names.contains(name) {
            group.names.push(name.clone());
        }
    }
}

/// Tag definitions needed to expand some tag groups, by (dictionary id,
/// name before `:`).
pub(crate) struct TagLookup {
    records: HashMap<(i64, String), Option<TagRecord>>,
}

impl TagLookup {
    /// Looks up every name of `groups` in one query.
    pub fn load<'a>(
        store: &Store,
        groups: impl IntoIterator<Item = &'a TagGroup>,
    ) -> Result<TagLookup, StoreError> {
        let mut keys: Vec<(i64, String)> = Vec::new();
        let mut seen: HashMap<(i64, String), ()> = HashMap::new();
        for group in groups {
            for name in &group.names {
                let key = (group.dictionary_id, name_base(name).to_string());
                if seen.insert(key.clone(), ()).is_none() {
                    keys.push(key);
                }
            }
        }
        let found = store.find_tags(&keys)?;
        Ok(TagLookup {
            records: keys.into_iter().zip(found).collect(),
        })
    }

    pub fn record(&self, dictionary_id: i64, name: &str) -> Option<&TagRecord> {
        self.records
            .get(&(dictionary_id, name_base(name).to_string()))
            .and_then(Option::as_ref)
    }

    /// _expandTagGroups + _groupTags for one target: its tags in the
    /// registry's order, similar tags merged, sorted by order then name.
    pub fn expand(&self, registry: &TagRegistry, groups: &[TagGroup]) -> Vec<Tag> {
        let mut pairs: Vec<(usize, &TagGroup, &String)> = groups
            .iter()
            .flat_map(|g| {
                g.names
                    .iter()
                    .map(move |n| (registry.rank_of(g.dictionary_id, n), g, n))
            })
            .collect();
        pairs.sort_by_key(|(rank, _, _)| *rank);
        let mut tags: Vec<Tag> = pairs
            .into_iter()
            .map(|(_, group, name)| {
                create_tag(
                    self.record(group.dictionary_id, name),
                    name,
                    &group.dictionary,
                )
            })
            .collect();
        if tags.len() > 1 {
            merge_similar_tags(&mut tags);
            tags.sort_by(|a, b| {
                a.order
                    .partial_cmp(&b.order)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| collation::compare(&a.name, &b.name))
            });
        }
        tags
    }
}

/// _getNameBase: the part of a tag name before the first `:`.
pub(crate) fn name_base(name: &str) -> &str {
    name.split_once(':').map_or(name, |(base, _)| base)
}

fn create_tag(record: Option<&TagRecord>, name: &str, dictionary: &str) -> Tag {
    let (category, order, score, notes) = match record {
        Some(r) => (r.category.as_str(), r.order, r.score, r.notes.as_str()),
        None => ("", 0.0, 0.0, ""),
    };
    Tag {
        name: name.to_string(),
        category: if category.is_empty() {
            "default".to_string()
        } else {
            category.to_string()
        },
        order,
        score,
        content: if notes.is_empty() {
            Vec::new()
        } else {
            vec![notes.to_string()]
        },
        dictionaries: vec![dictionary.to_string()],
        redundant: false,
    }
}

/// _mergeSimilarTags: tags with the same name and category become one.
fn merge_similar_tags(tags: &mut Vec<Tag>) {
    let mut i = 0;
    while i < tags.len() {
        let mut j = i + 1;
        while j < tags.len() {
            let same = match (tags.get(i), tags.get(j)) {
                (Some(a), Some(b)) => a.name == b.name && a.category == b.category,
                _ => false,
            };
            if !same {
                j += 1;
                continue;
            }
            let other = tags.remove(j);
            if let Some(tag) = tags.get_mut(i) {
                tag.order = tag.order.min(other.order);
                tag.score = tag.score.max(other.score);
                tag.dictionaries.extend(other.dictionaries);
                for item in other.content {
                    if !tag.content.contains(&item) {
                        tag.content.push(item);
                    }
                }
            }
        }
        i += 1;
    }
}

/// _flagRedundantDefinitionTags: a definition's part-of-speech tags are
/// redundant when the previous definition of the same dictionary has the
/// same ones.
pub(crate) fn flag_redundant_definition_tags(definitions: &mut [Definition]) {
    let mut last_dictionary: Option<i64> = None;
    let mut last_part_of_speech: Option<Vec<String>> = None;
    for definition in definitions {
        let part_of_speech = names_with_category(&definition.tags, "partOfSpeech");
        if last_dictionary != Some(definition.dictionary_id) {
            last_dictionary = Some(definition.dictionary_id);
            last_part_of_speech = None;
        }
        // Yomitan resets to '', which no key equals.
        if last_part_of_speech.as_ref() == Some(&part_of_speech) {
            for tag in &mut definition.tags {
                if tag.category == "partOfSpeech" {
                    tag.redundant = true;
                }
            }
        } else {
            last_part_of_speech = Some(part_of_speech);
        }
    }
}

/// _getTagNamesWithCategory: names of the tags in `category`, sorted (by
/// UTF-16 code units, as JavaScript's default sort).
fn names_with_category(tags: &[Tag], category: &str) -> Vec<String> {
    let mut names: Vec<String> = tags
        .iter()
        .filter(|t| t.category == category)
        .map(|t| t.name.clone())
        .collect();
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    names
}
