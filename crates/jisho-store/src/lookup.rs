// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term queries: matching on the small `terms` columns, then loading
//! glossaries only for the entries a caller keeps.

use std::collections::HashMap;

use rusqlite::Row;
use serde_json::Value;

use crate::error::StoreError;
use crate::records::{BulkHit, MatchSource, TermHit, TermRecord, split_list};
use crate::store::Store;

/// SQLite's default limit on bound parameters is 32766; batches stay well
/// below it.
const MAX_BATCH: usize = 4000;

impl Store {
    /// Terms whose expression or reading equals `text` exactly, from enabled
    /// dictionaries, ordered by dictionary priority then import order, at
    /// most `limit` of them, with glossaries.
    pub fn lookup_exact(&self, text: &str, limit: usize) -> Result<Vec<TermRecord>, StoreError> {
        let hits = self.find_terms(&[text.to_string()], limit)?;
        self.with_glossaries(hits)
    }

    /// Terms whose expression or reading equals any of `texts`, from enabled
    /// dictionaries, ordered by dictionary priority then import order,
    /// without glossaries, at most `max_hits` of them. Duplicate texts are
    /// fine. This is the query a scan issues for all of its candidate
    /// strings at once.
    pub fn find_terms(
        &self,
        texts: &[String],
        max_hits: usize,
    ) -> Result<Vec<TermHit>, StoreError> {
        let limit = i64::try_from(max_hits).unwrap_or(i64::MAX);
        let mut hits = Vec::new();
        for batch in texts.chunks(MAX_BATCH) {
            let mut stmt = self.conn.prepare(&find_terms_sql(batch.len()))?;
            let mut params: Vec<&dyn rusqlite::ToSql> =
                batch.iter().map(|t| t as &dyn rusqlite::ToSql).collect();
            params.push(&limit);
            let rows = stmt.query_map(params.as_slice(), read_hit)?;
            for row in rows {
                hits.push(row?);
            }
        }
        if texts.len() > MAX_BATCH {
            hits.sort_by_key(|h| (h.dictionary_priority, h.id));
            hits.dedup_by_key(|h| h.id);
            hits.truncate(max_hits);
        }
        Ok(hits)
    }

    /// Exact matches for each of `texts` (which must be distinct), as
    /// Yomitan's findTermsBulk returns them from IndexedDB: every row once,
    /// claimed by the first request that finds it, where requests go text by
    /// text and, for each text, the expression index before the reading
    /// index; results in that request order, ids ascending within a request.
    /// At most `max_hits` rows are considered (a safety cap).
    pub fn find_terms_bulk(
        &self,
        texts: &[String],
        max_hits: usize,
    ) -> Result<Vec<BulkHit>, StoreError> {
        let positions: HashMap<&str, usize> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| (t.as_str(), i))
            .collect();
        let mut claimed = Vec::new();
        for hit in self.find_terms(texts, max_hits)? {
            let by_term = positions
                .get(hit.term.as_str())
                .map(|&i| (i, MatchSource::Term));
            let by_reading = positions
                .get(hit.reading.as_str())
                .map(|&i| (i, MatchSource::Reading));
            let first = match (by_term, by_reading) {
                (Some(t), Some(r)) => Some(if r.0 < t.0 { r } else { t }),
                (t, r) => t.or(r),
            };
            if let Some((item_index, match_source)) = first {
                claimed.push(BulkHit {
                    hit,
                    item_index,
                    match_source,
                });
            }
        }
        claimed.sort_by_key(|b| {
            (
                b.item_index,
                b.match_source == MatchSource::Reading,
                b.hit.id,
            )
        });
        Ok(claimed)
    }

    /// Like [`Store::with_glossaries`], but stops before the total glossary
    /// size (stored JSON bytes) would exceed `max_bytes`. Returns the records
    /// and whether any hit was dropped.
    pub fn with_glossaries_within(
        &self,
        hits: Vec<TermHit>,
        max_bytes: usize,
    ) -> Result<(Vec<TermRecord>, bool), StoreError> {
        let ids: Vec<i64> = hits.iter().map(|h| h.id).collect();
        let (mut glossaries, truncated) = self.glossaries_within(&ids, max_bytes)?;
        let mut records = Vec::with_capacity(glossaries.len());
        for hit in hits {
            match glossaries.remove(&hit.id) {
                Some(glossary) => records.push(TermRecord { hit, glossary }),
                None => break,
            }
        }
        Ok((records, truncated))
    }

    /// Glossaries of the terms `ids`, in order, stopping before the total
    /// stored size would exceed `max_bytes`. Sizes are read before any
    /// glossary is loaded, so an over-budget glossary never gets into
    /// memory. Returns the glossaries by id and whether any id was dropped.
    pub fn glossaries_within(
        &self,
        ids: &[i64],
        max_bytes: usize,
    ) -> Result<(HashMap<i64, Vec<Value>>, bool), StoreError> {
        let mut sizes: HashMap<i64, usize> = HashMap::with_capacity(ids.len());
        for batch in ids.chunks(MAX_BATCH) {
            let placeholders = vec!["?"; batch.len()].join(", ");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT id, length(CAST(glossary AS BLOB)) FROM term_glossaries
                 WHERE id IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(batch.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (id, size) = row?;
                sizes.insert(id, usize::try_from(size).unwrap_or(usize::MAX));
            }
        }
        let mut kept = Vec::with_capacity(ids.len());
        let mut used = 0usize;
        for &id in ids {
            let size = sizes.get(&id).copied().unwrap_or(0);
            if used.saturating_add(size) > max_bytes {
                break;
            }
            used += size;
            kept.push(id);
        }
        let truncated = kept.len() < ids.len();
        Ok((self.load_glossaries(&kept)?, truncated))
    }

    /// Loads the glossaries of `hits`, keeping their order.
    pub fn with_glossaries(&self, hits: Vec<TermHit>) -> Result<Vec<TermRecord>, StoreError> {
        let ids: Vec<i64> = hits.iter().map(|h| h.id).collect();
        let mut glossaries = self.load_glossaries(&ids)?;
        Ok(hits
            .into_iter()
            .map(|hit| {
                let glossary = glossaries.remove(&hit.id).unwrap_or_default();
                TermRecord { hit, glossary }
            })
            .collect())
    }

    fn load_glossaries(&self, ids: &[i64]) -> Result<HashMap<i64, Vec<Value>>, StoreError> {
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
        Ok(glossaries)
    }

    /// SQLite's query plan (`EXPLAIN QUERY PLAN`) for [`Store::find_terms`]
    /// with `n` texts, one line per step. For benchmarks and diagnostics.
    pub fn explain_find_terms(&self, n: usize) -> Result<Vec<String>, StoreError> {
        let n = n.clamp(1, MAX_BATCH);
        let sql = format!("EXPLAIN QUERY PLAN {}", find_terms_sql(n));
        let mut stmt = self.conn.prepare(&sql)?;
        let mut texts: Vec<String> = (0..n).map(|i| format!("x{i}")).collect();
        texts.push("1".to_string());
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
/// expression and the reading lists, and `?{n+1}` is the row limit.
fn find_terms_sql(n: usize) -> String {
    let list = (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT t.id, t.dictionary, d.title, d.priority, t.expression, t.reading,
                t.definition_tags, t.rules, t.score, t.sequence, t.term_tags, t.form_of,
                t.content_count
         FROM terms t JOIN dictionaries d ON d.id = t.dictionary
         WHERE d.enabled AND (t.expression IN ({list}) OR t.reading IN ({list}))
         ORDER BY d.priority, t.id
         LIMIT ?{}",
        n + 1
    )
}

fn read_hit(row: &Row<'_>) -> rusqlite::Result<TermHit> {
    let definition_tags: String = row.get(6)?;
    let rules: String = row.get(7)?;
    let term_tags: String = row.get(10)?;
    let form_of_json: Option<String> = row.get(11)?;
    let form_of = match form_of_json {
        None => Vec::new(),
        Some(text) => parse_form_of(&text).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, e.into())
        })?,
    };
    let content_count: i64 = row.get(12)?;
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
        form_of,
        content_count: u32::try_from(content_count).unwrap_or(u32::MAX),
    })
}

/// Parses the `form_of` column: a JSON array of `[term, [rules]]`.
fn parse_form_of(text: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let items: Vec<(String, Vec<String>)> =
        serde_json::from_str(text).map_err(|e| format!("form_of: {e}"))?;
    Ok(items)
}
