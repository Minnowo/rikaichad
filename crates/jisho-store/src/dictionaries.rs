// SPDX-License-Identifier: AGPL-3.0-or-later
//! Listing and removing imported dictionaries.

use jisho_format::Index;
use rusqlite::{Row, TransactionBehavior};

use crate::error::StoreError;
use crate::records::{Counts, DictionaryInfo};
use crate::store::Store;

/// Tables whose rows belong to a dictionary, cleared on removal.
const DATA_TABLES: [&str; 5] = ["terms", "term_meta", "kanji", "kanji_meta", "tags"];

impl Store {
    /// All imported dictionaries in the user's order (priority, then id).
    pub fn dictionaries(&self) -> Result<Vec<DictionaryInfo>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, priority, enabled, index_json, styles IS NOT NULL, source_name,
                    source_sha256, imported_at, term_count, term_meta_count, kanji_count,
                    kanji_meta_count, tag_count
             FROM dictionaries ORDER BY priority, id",
        )?;
        let rows = stmt.query_map([], read_dictionary)?;
        let mut result = Vec::new();
        for row in rows {
            let raw = row?;
            result.push(DictionaryInfo {
                id: raw.id,
                priority: raw.priority,
                enabled: raw.enabled,
                index: Index::parse(&raw.index_json)?,
                has_styles: raw.has_styles,
                source_name: raw.source_name,
                source_sha256: raw.source_sha256,
                imported_at: raw.imported_at,
                counts: raw.counts,
            });
        }
        Ok(result)
    }

    /// Finds a dictionary by its title.
    pub fn dictionary_by_title(&self, title: &str) -> Result<DictionaryInfo, StoreError> {
        self.dictionaries()?
            .into_iter()
            .find(|d| d.index.title == title)
            .ok_or_else(|| StoreError::NotFound {
                what: format!("titled {title:?}"),
            })
    }

    /// Removes the dictionary with `id` and all its rows, in one
    /// transaction. Large dictionaries take a while (rows are deleted, then
    /// the freed pages stay in the file for reuse).
    pub fn remove(&mut self, id: i64) -> Result<(), StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = tx.execute("DELETE FROM dictionaries WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(StoreError::NotFound {
                what: format!("with id {id}"),
            });
        }
        for table in DATA_TABLES {
            tx.execute(&format!("DELETE FROM {table} WHERE dictionary = ?1"), [id])?;
        }
        tx.commit()?;
        self.checkpoint()?;
        Ok(())
    }
}

/// A `dictionaries` row before its index JSON is parsed (parsing errors
/// are store errors, which a row mapper cannot return).
struct RawDictionary {
    id: i64,
    priority: i64,
    enabled: bool,
    index_json: String,
    has_styles: bool,
    source_name: String,
    source_sha256: String,
    imported_at: i64,
    counts: Counts,
}

fn read_dictionary(row: &Row<'_>) -> rusqlite::Result<RawDictionary> {
    let count = |i: usize| -> rusqlite::Result<u64> {
        let value: i64 = row.get(i)?;
        Ok(u64::try_from(value).unwrap_or(0))
    };
    Ok(RawDictionary {
        id: row.get(0)?,
        priority: row.get(1)?,
        enabled: row.get(2)?,
        index_json: row.get(3)?,
        has_styles: row.get(4)?,
        source_name: row.get(5)?,
        source_sha256: row.get(6)?,
        imported_at: row.get(7)?,
        counts: Counts {
            terms: count(8)?,
            term_meta: count(9)?,
            kanji: count(10)?,
            kanji_meta: count(11)?,
            tags: count(12)?,
        },
    })
}
