// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lookup options, mirroring the parts of Yomitan's FindTermsOptions that
//! jisho supports, with Yomitan's defaults.

use std::collections::HashMap;

use jisho_store::{Store, StoreError};

/// Options for [`crate::find_terms`].
#[derive(Debug, Clone)]
pub struct FindTermsOptions {
    /// Cut the text at the first character that is not Japanese, Chinese or
    /// Korean (Yomitan default: true).
    pub remove_non_japanese_characters: bool,
    /// Undo inflections (Yomitan default: true).
    pub deinflect: bool,
    /// Entries whose reading equals this sort first (empty: no effect).
    pub primary_reading: String,
    /// Safety cap on matched rows per lookup.
    pub max_hits: usize,
}

impl Default for FindTermsOptions {
    fn default() -> Self {
        FindTermsOptions {
            remove_non_japanese_characters: true,
            deinflect: true,
            primary_reading: String::new(),
            max_hits: 50_000,
        }
    }
}

/// Per-dictionary lookup settings (Yomitan's FindTermDictionary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryOptions {
    /// Position among enabled dictionaries; lower sorts first.
    pub index: usize,
    /// Display name (Yomitan's alias; jisho uses the title).
    pub alias: String,
    /// Title, for results.
    pub title: String,
    /// Only accept deinflections whose conditions match the entry's part
    /// of speech (Yomitan default: true).
    pub parts_of_speech_filter: bool,
    /// Follow the dictionary's own deinflection references (default: true).
    pub use_deinflections: bool,
}

/// The enabled dictionaries of a store, by dictionary id.
#[derive(Debug, Clone, Default)]
pub struct EnabledDictionaries {
    pub by_id: HashMap<i64, DictionaryOptions>,
}

impl EnabledDictionaries {
    /// Reads the enabled dictionaries of `store` in their order, with
    /// default settings.
    pub fn from_store(store: &Store) -> Result<EnabledDictionaries, StoreError> {
        let mut by_id = HashMap::new();
        let enabled = store.dictionaries()?.into_iter().filter(|d| d.enabled);
        for (index, info) in enabled.enumerate() {
            by_id.insert(
                info.id,
                DictionaryOptions {
                    index,
                    alias: info.index.title.clone(),
                    title: info.index.title,
                    parts_of_speech_filter: true,
                    use_deinflections: true,
                },
            );
        }
        Ok(EnabledDictionaries { by_id })
    }
}
