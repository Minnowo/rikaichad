// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term queries: matching on the small `terms` columns, then loading
//! glossaries only for the entries a caller keeps.

use std::collections::HashMap;

use rusqlite::Row;
use serde_json::Value;

use crate::error::StoreError;
use crate::records::{TermHit, TermRecord, split_list};
use crate::store::Store;

/// SQLite's default limit on bound parameters is 32766; batches stay well
/// below it.
const MAX_BATCH: usize = 4000;

impl Store {
    /// Terms whose expression or reading equals `text` exactly, from enabled
    /// dictionaries, ordered by dictionary priority then import order, at
    /// most `limit` of them, with glossaries.
    pub fn lookup_exact(&self, text: &str, limit: usize) -> Result<Vec<TermRecord>, StoreError> {
        let mut hits = self.find_terms(&[text.to_string()])?;
        hits.truncate(limit);
        self.with_glossaries(hits)
    }

    /// Terms whose expression or reading equals any of `texts`, from enabled
    /// dictionaries, ordered by dictionary priority then import order,
    /// without glossaries. Duplicate texts are fine. This is the query a
    /// scan issues for all of its candidate strings at once.
    pub fn find_terms(&self, texts: &[String]) -> Result<Vec<TermHit>, StoreError> {
        let mut hits = Vec::new();
        for batch in texts.chunks(MAX_BATCH) {
            let mut stmt = self.conn.prepare(&find_terms_sql(batch.len()))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(batch.iter()), read_hit)?;
            for row in rows {
                hits.push(row?);
            }
        }
        if texts.len() > MAX_BATCH {
            hits.sort_by_key(|h| (h.dictionary_priority, h.id));
            hits.dedup_by_key(|h| h.id);
        }
        Ok(hits)
    }

    /// Loads the glossaries of `hits`, keeping their order.
    pub fn with_glossaries(&self, hits: Vec<TermHit>) -> Result<Vec<TermRecord>, StoreError> {
        let ids: Vec<i64> = hits.iter().map(|h| h.id).collect();
        let mut glossaries: HashMap<i64, Vec<Value>> = HashMap::with_capacity(ids.len());
        for batch in ids.chunks(MAX_BATCH) {
            let placeholders = vec!["?"; batch.len()].join(", ");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT id, glossary FROM term_glossaries WHERE id IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(batch.iter()), |row| {
                let id: i64 = row.get(0)?;
                let text: String = row.get(1)?;
                Ok((id, text))
            })?;
            for row in rows {
                let (id, text) = row?;
                let glossary = serde_json::from_str(&text).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                glossaries.insert(id, glossary);
            }
        }
        Ok(hits
            .into_iter()
            .map(|hit| {
                let glossary = glossaries.remove(&hit.id).unwrap_or_default();
                TermRecord { hit, glossary }
            })
            .collect())
    }

    /// SQLite's query plan (`EXPLAIN QUERY PLAN`) for [`Store::find_terms`]
    /// with `n` texts, one line per step. For benchmarks and diagnostics.
    pub fn explain_find_terms(&self, n: usize) -> Result<Vec<String>, StoreError> {
        let n = n.clamp(1, MAX_BATCH);
        let sql = format!("EXPLAIN QUERY PLAN {}", find_terms_sql(n));
        let mut stmt = self.conn.prepare(&sql)?;
        let texts: Vec<String> = (0..n).map(|i| format!("x{i}")).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(texts.iter()), |row| {
            row.get::<_, String>(3)
        })?;
        let mut lines = Vec::new();
        for row in rows {
            lines.push(row?);
        }
        Ok(lines)
    }
}

/// SQL matching `n` texts; the same placeholders (`?1..?n`) serve both the
/// expression and the reading lists.
fn find_terms_sql(n: usize) -> String {
    let list = (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT t.id, t.dictionary, d.title, d.priority, t.expression, t.reading,
                t.definition_tags, t.rules, t.score, t.sequence, t.term_tags
         FROM terms t JOIN dictionaries d ON d.id = t.dictionary
         WHERE d.enabled AND (t.expression IN ({list}) OR t.reading IN ({list}))
         ORDER BY d.priority, t.id"
    )
}

fn read_hit(row: &Row<'_>) -> rusqlite::Result<TermHit> {
    let definition_tags: String = row.get(6)?;
    let rules: String = row.get(7)?;
    let term_tags: String = row.get(10)?;
    Ok(TermHit {
        id: row.get(0)?,
        dictionary_id: row.get(1)?,
        dictionary_title: row.get(2)?,
        dictionary_priority: row.get(3)?,
        term: row.get(4)?,
        reading: row.get(5)?,
        definition_tags: split_list(&definition_tags),
        rules: split_list(&rules),
        score: row.get(8)?,
        sequence: row.get(9)?,
        term_tags: split_list(&term_tags),
    })
}
