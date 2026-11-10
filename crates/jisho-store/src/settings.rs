// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lookup settings kept in the store, so that every client of one store
//! (CLI, server) looks up the same way: dictionary order, enable flags,
//! per-dictionary options (Yomitan's dictionary settings) and the global
//! options Yomitan keeps under `general` (main and sort-frequency
//! dictionary).

use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::error::StoreError;
use crate::store::Store;

/// Per-dictionary lookup options (Yomitan's dictionary settings, same
/// defaults).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionarySettings {
    /// Display name in results; `None` means the title.
    pub alias: Option<String>,
    /// In merge mode, also search this dictionary for the headwords of the
    /// main dictionary's entries (default false).
    pub allow_secondary_searches: bool,
    /// Only accept deinflections whose conditions fit the entry's part of
    /// speech (default true).
    pub parts_of_speech_filter: bool,
    /// Follow the dictionary's own deinflection references (default true).
    pub use_deinflections: bool,
}

impl Default for DictionarySettings {
    fn default() -> Self {
        DictionarySettings {
            alias: None,
            allow_secondary_searches: false,
            parts_of_speech_filter: true,
            use_deinflections: true,
        }
    }
}

/// Order of the sort-frequency dictionary's values (Yomitan's
/// `sortFrequencyDictionaryOrder`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrequencyOrder {
    /// Smaller values first (rank-based lists).
    Ascending,
    /// Larger values first (occurrence counts); Yomitan's default.
    #[default]
    Descending,
}

impl FrequencyOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            FrequencyOrder::Ascending => "ascending",
            FrequencyOrder::Descending => "descending",
        }
    }

    pub fn parse(text: &str) -> Option<FrequencyOrder> {
        match text {
            "ascending" => Some(FrequencyOrder::Ascending),
            "descending" => Some(FrequencyOrder::Descending),
            _ => None,
        }
    }
}

/// Store-wide lookup settings. Dictionaries are named by title, as in
/// Yomitan, so a setting survives removing and re-importing a dictionary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LookupSettings {
    /// Dictionary whose sequence numbers group entries in merge mode.
    /// Set at import to the first sequenced dictionary when unset.
    pub main_dictionary: Option<String>,
    /// Dictionary whose frequencies order the results.
    pub sort_frequency_dictionary: Option<String>,
    pub sort_frequency_order: FrequencyOrder,
}

/// A dictionary as lookups need it: identity, order, flags, settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupDictionary {
    pub id: i64,
    pub title: String,
    pub enabled: bool,
    pub settings: DictionarySettings,
    /// `frequencyMode` from index.json.
    pub frequency_mode: Option<String>,
}

const MAIN_DICTIONARY: &str = "main_dictionary";
const SORT_FREQUENCY_DICTIONARY: &str = "sort_frequency_dictionary";
const SORT_FREQUENCY_ORDER: &str = "sort_frequency_order";

impl Store {
    /// Every dictionary in lookup order (priority, then import order),
    /// enabled or not. Cheap: no index.json parsing.
    pub fn lookup_dictionaries(&self) -> Result<Vec<LookupDictionary>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, enabled, alias, allow_secondary_searches, parts_of_speech_filter,
                    use_deinflections, json_extract(index_json, '$.frequencyMode')
             FROM dictionaries ORDER BY priority, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(LookupDictionary {
                id: row.get(0)?,
                title: row.get(1)?,
                enabled: row.get(2)?,
                settings: DictionarySettings {
                    alias: row.get(3)?,
                    allow_secondary_searches: row.get(4)?,
                    parts_of_speech_filter: row.get(5)?,
                    use_deinflections: row.get(6)?,
                },
                frequency_mode: row.get(7)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Enables or disables dictionary `id`.
    pub fn set_enabled(&mut self, id: i64, enabled: bool) -> Result<(), StoreError> {
        let changed = self.conn.execute(
            "UPDATE dictionaries SET enabled = ?2 WHERE id = ?1",
            params![id, enabled],
        )?;
        found(changed, id)
    }

    /// Replaces the settings of dictionary `id`.
    pub fn set_dictionary_settings(
        &mut self,
        id: i64,
        settings: &DictionarySettings,
    ) -> Result<(), StoreError> {
        let changed = self.conn.execute(
            "UPDATE dictionaries SET alias = ?2, allow_secondary_searches = ?3,
                 parts_of_speech_filter = ?4, use_deinflections = ?5
             WHERE id = ?1",
            params![
                id,
                settings.alias,
                settings.allow_secondary_searches,
                settings.parts_of_speech_filter,
                settings.use_deinflections
            ],
        )?;
        found(changed, id)
    }

    /// Replaces the settings of dictionary `id` and, if given, its enable
    /// flag, in one transaction.
    pub fn update_dictionary(
        &mut self,
        id: i64,
        settings: &DictionarySettings,
        enabled: Option<bool>,
    ) -> Result<(), StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE dictionaries SET alias = ?2, allow_secondary_searches = ?3,
                 parts_of_speech_filter = ?4, use_deinflections = ?5,
                 enabled = COALESCE(?6, enabled)
             WHERE id = ?1",
            params![
                id,
                settings.alias,
                settings.allow_secondary_searches,
                settings.parts_of_speech_filter,
                settings.use_deinflections,
                enabled
            ],
        )?;
        found(changed, id)?;
        tx.commit()?;
        Ok(())
    }

    /// Puts the dictionaries `ids` first, in that order; the others follow
    /// in their current order. Fails if an id is unknown or repeated.
    pub fn set_order(&mut self, ids: &[i64]) -> Result<(), StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current: Vec<i64> = Vec::new();
        {
            let mut stmt = tx.prepare("SELECT id FROM dictionaries ORDER BY priority, id")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            for row in rows {
                current.push(row?);
            }
        }
        for (i, id) in ids.iter().enumerate() {
            if !current.contains(id) {
                return Err(StoreError::NotFound {
                    what: format!("with id {id}"),
                });
            }
            if ids.get(..i).is_some_and(|earlier| earlier.contains(id)) {
                return Err(StoreError::Invalid {
                    message: format!("dictionary {id} is listed twice"),
                });
            }
        }
        let order = ids
            .iter()
            .copied()
            .chain(current.into_iter().filter(|id| !ids.contains(id)));
        for (priority, id) in order.enumerate() {
            tx.execute(
                "UPDATE dictionaries SET priority = ?2 WHERE id = ?1",
                params![id, i64::try_from(priority).unwrap_or(i64::MAX)],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The store-wide lookup settings.
    pub fn lookup_settings(&self) -> Result<LookupSettings, StoreError> {
        let order = self.meta_value(SORT_FREQUENCY_ORDER)?;
        Ok(LookupSettings {
            main_dictionary: self.meta_value(MAIN_DICTIONARY)?,
            sort_frequency_dictionary: self.meta_value(SORT_FREQUENCY_DICTIONARY)?,
            sort_frequency_order: order
                .as_deref()
                .and_then(FrequencyOrder::parse)
                .unwrap_or_default(),
        })
    }

    /// Replaces the store-wide lookup settings. Titles are not checked
    /// against the imported dictionaries (Yomitan does not either).
    pub fn set_lookup_settings(&mut self, settings: &LookupSettings) -> Result<(), StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pairs = [
            (MAIN_DICTIONARY, settings.main_dictionary.as_deref()),
            (
                SORT_FREQUENCY_DICTIONARY,
                settings.sort_frequency_dictionary.as_deref(),
            ),
            (
                SORT_FREQUENCY_ORDER,
                Some(settings.sort_frequency_order.as_str()),
            ),
        ];
        for (key, value) in pairs {
            match value {
                Some(value) => tx.execute(
                    "INSERT INTO meta (key, value) VALUES (?1, ?2)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )?,
                None => tx.execute("DELETE FROM meta WHERE key = ?1", [key])?,
            };
        }
        tx.commit()?;
        Ok(())
    }

    fn meta_value(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }
}

fn found(changed: usize, id: i64) -> Result<(), StoreError> {
    if changed == 0 {
        return Err(StoreError::NotFound {
            what: format!("with id {id}"),
        });
    }
    Ok(())
}
