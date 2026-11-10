// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term queries.

use rusqlite::Row;

use crate::error::StoreError;
use crate::records::{TermRecord, split_list};
use crate::store::Store;

impl Store {
    /// Terms whose expression or reading equals `text` exactly, from enabled
    /// dictionaries, ordered by dictionary priority then import order, at
    /// most `limit` of them.
    pub fn lookup_exact(&self, text: &str, limit: usize) -> Result<Vec<TermRecord>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.dictionary, d.title, d.priority, t.expression, t.reading,
                    t.definition_tags, t.rules, t.score, t.glossary, t.sequence, t.term_tags
             FROM terms t JOIN dictionaries d ON d.id = t.dictionary
             WHERE d.enabled AND (t.expression = ?1 OR t.reading = ?1)
             ORDER BY d.priority, t.id
             LIMIT ?2",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = stmt.query_map(rusqlite::params![text, limit], read_term)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

fn read_term(row: &Row<'_>) -> rusqlite::Result<TermRecord> {
    let glossary_json: String = row.get(9)?;
    let glossary = serde_json::from_str(&glossary_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let definition_tags: String = row.get(6)?;
    let rules: String = row.get(7)?;
    let term_tags: String = row.get(11)?;
    Ok(TermRecord {
        id: row.get(0)?,
        dictionary_id: row.get(1)?,
        dictionary_title: row.get(2)?,
        dictionary_priority: row.get(3)?,
        term: row.get(4)?,
        reading: row.get(5)?,
        definition_tags: split_list(&definition_tags),
        rules: split_list(&rules),
        score: row.get(8)?,
        glossary,
        sequence: row.get(10)?,
        term_tags: split_list(&term_tags),
    })
}
