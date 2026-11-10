// Ported from Yomitan ext/js/background/backend.js
// (_getTranslatorFindTermsOptions, _getTranslatorEnabledDictionaryMap) at
// ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Lookup options, mirroring the parts of Yomitan's FindTermsOptions that
//! jisho supports, with Yomitan's defaults.

use std::collections::HashMap;

use jisho_store::{FrequencyOrder, Store, StoreError};

/// How results are grouped (Yomitan's `general.resultOutputMode`, plus
/// the internal "simple" mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultMode {
    /// One headword and one definition per entry.
    Split,
    /// Entries with the same term, reading and inflections grouped; Yomitan's
    /// default.
    #[default]
    Group,
    /// Entries of the main dictionary grouped by sequence number, other
    /// dictionaries' entries attached by headword.
    Merge,
    /// Entries with the same term and inflections grouped.
    Term,
    /// Like split, without tags and metadata, except the sort-frequency
    /// dictionary's metadata when one is set (cheapest).
    Simple,
}

impl ResultMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ResultMode::Split => "split",
            ResultMode::Group => "group",
            ResultMode::Merge => "merge",
            ResultMode::Term => "term",
            ResultMode::Simple => "simple",
        }
    }

    pub fn parse(text: &str) -> Option<ResultMode> {
        match text {
            "split" => Some(ResultMode::Split),
            "group" => Some(ResultMode::Group),
            "merge" => Some(ResultMode::Merge),
            "term" => Some(ResultMode::Term),
            "simple" => Some(ResultMode::Simple),
            _ => None,
        }
    }
}

/// Options for [`crate::find_terms`].
#[derive(Debug, Clone)]
pub struct FindTermsOptions {
    pub mode: ResultMode,
    /// Cut the text at the first character that is not Japanese, Chinese or
    /// Korean. Yomitan sets this to `!scanning.alphanumeric`, whose default
    /// is true, so the effective default is false (and romaji input works).
    pub remove_non_japanese_characters: bool,
    /// Undo inflections (Yomitan default: true).
    pub deinflect: bool,
    /// Entries whose reading equals this sort first (empty: no effect).
    pub primary_reading: String,
    /// Safety cap on matched rows per lookup.
    pub max_hits: usize,
    /// Keep only this many entries (Yomitan's maxResults, applied after
    /// sorting). Tags and metadata are then computed for the kept entries
    /// only, which gives the same result faster.
    pub limit: Option<usize>,
}

impl Default for FindTermsOptions {
    fn default() -> Self {
        FindTermsOptions {
            mode: ResultMode::default(),
            remove_non_japanese_characters: false,
            deinflect: true,
            primary_reading: String::new(),
            max_hits: 50_000,
            limit: None,
        }
    }
}

/// Per-dictionary lookup settings (Yomitan's FindTermDictionary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryOptions {
    /// Position among enabled dictionaries; lower sorts first.
    pub index: usize,
    /// Display name (Yomitan's alias; the title unless set).
    pub alias: String,
    /// Title, for results.
    pub title: String,
    /// In merge mode, search this dictionary for the main dictionary's
    /// headwords (Yomitan default: false).
    pub allow_secondary_searches: bool,
    /// Only accept deinflections whose conditions match the entry's part
    /// of speech (Yomitan default: true).
    pub parts_of_speech_filter: bool,
    /// Follow the dictionary's own deinflection references (default: true).
    pub use_deinflections: bool,
}

/// The dictionaries a lookup uses and the store-wide settings that go with
/// them.
#[derive(Debug, Clone, Default)]
pub struct EnabledDictionaries {
    /// Enabled dictionaries by id.
    pub by_id: HashMap<i64, DictionaryOptions>,
    /// `frequencyMode` of every dictionary that has one.
    pub frequency_modes: HashMap<i64, String>,
    /// The main dictionary (merge mode) by id and title, if imported.
    pub main_dictionary: Option<(i64, String)>,
    /// The dictionary whose frequencies order results, if imported.
    pub sort_frequency_dictionary: Option<i64>,
    pub sort_frequency_order: FrequencyOrder,
    /// Dictionaries whose definitions are dropped from results (merge mode
    /// with a disabled main dictionary, see [`Self::for_mode`]).
    pub excluded: Option<i64>,
}

impl EnabledDictionaries {
    /// Reads the dictionaries and lookup settings of `store`.
    pub fn from_store(store: &Store) -> Result<EnabledDictionaries, StoreError> {
        let settings = store.lookup_settings()?;
        let mut result = EnabledDictionaries {
            sort_frequency_order: settings.sort_frequency_order,
            ..EnabledDictionaries::default()
        };
        for dictionary in store.lookup_dictionaries()? {
            if let Some(mode) = &dictionary.frequency_mode {
                result.frequency_modes.insert(dictionary.id, mode.clone());
            }
            if settings.main_dictionary.as_deref() == Some(dictionary.title.as_str()) {
                result.main_dictionary = Some((dictionary.id, dictionary.title.clone()));
            }
            if settings.sort_frequency_dictionary.as_deref() == Some(dictionary.title.as_str()) {
                result.sort_frequency_dictionary = Some(dictionary.id);
            }
            if !dictionary.enabled {
                continue;
            }
            let index = result.by_id.len();
            result.by_id.insert(
                dictionary.id,
                DictionaryOptions {
                    index,
                    alias: dictionary
                        .settings
                        .alias
                        .filter(|a| !a.is_empty())
                        .unwrap_or_else(|| dictionary.title.clone()),
                    title: dictionary.title,
                    allow_secondary_searches: dictionary.settings.allow_secondary_searches,
                    parts_of_speech_filter: dictionary.settings.parts_of_speech_filter,
                    use_deinflections: dictionary.settings.use_deinflections,
                },
            );
        }
        Ok(result)
    }

    /// The dictionaries as Yomitan's backend passes them for `mode`: in
    /// merge mode a disabled main dictionary is searched anyway, last, and
    /// its definitions are dropped from the results.
    pub fn for_mode(&self, mode: ResultMode) -> EnabledDictionaries {
        let mut result = self.clone();
        if mode != ResultMode::Merge {
            return result;
        }
        if let Some((id, title)) = &self.main_dictionary
            && !self.by_id.contains_key(id)
        {
            result.by_id.insert(
                *id,
                DictionaryOptions {
                    index: self.by_id.len(),
                    alias: title.clone(),
                    title: title.clone(),
                    allow_secondary_searches: false,
                    parts_of_speech_filter: true,
                    use_deinflections: true,
                },
            );
            result.excluded = Some(*id);
        }
        result
    }

    /// Dictionary ids in lookup order.
    pub(crate) fn ids(&self) -> Vec<i64> {
        let mut ids: Vec<(usize, i64)> = self.by_id.iter().map(|(id, d)| (d.index, *id)).collect();
        ids.sort_unstable();
        ids.into_iter().map(|(_, id)| id).collect()
    }

    /// Yomitan's _getDictionaryOrder: the index, or the number of enabled
    /// dictionaries for one that is not enabled.
    pub(crate) fn index_of(&self, id: i64) -> usize {
        self.by_id.get(&id).map_or(self.by_id.len(), |d| d.index)
    }

    /// Yomitan's _getDictionaryAlias: the alias, or the title.
    pub(crate) fn alias_of(&self, id: i64, title: &str) -> String {
        self.by_id
            .get(&id)
            .map_or_else(|| title.to_string(), |d| d.alias.clone())
    }

    /// Title of a searched dictionary (metadata only comes from those).
    pub(crate) fn title_of(&self, id: i64) -> String {
        self.by_id
            .get(&id)
            .map(|d| d.title.clone())
            .unwrap_or_default()
    }

    /// The disabled dictionary searched anyway (see [`Self::for_mode`]).
    pub(crate) fn extra_searched(&self) -> Option<i64> {
        self.excluded
    }
}
