// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term queries on the small `terms` columns (glossaries: glossaries.rs).

use std::collections::HashMap;

use rusqlite::Row;

use crate::error::StoreError;
use crate::records::{BulkHit, MatchSource, TermHit, TermRecord, split_list};
use crate::store::Store;

/// SQLite's default limit on bound parameters is 32766; batches stay well
/// below it.
pub(crate) const MAX_BATCH: usize = 4000;

impl Store {
    /// Terms whose expression or reading equals `text` exactly, from enabled
    /// dictionaries, ordered by dictionary priority then import order, at
    /// most `limit` of them, with glossaries.
    pub fn lookup_exact(&self, text: &str, limit: usize) -> Result<Vec<TermRecord>, StoreError> {
        let hits = self.find_terms(&[text.to_string()], None, limit)?;
        self.with_glossaries(hits)
    }

    /// Terms whose expression or reading equals any of `texts`, from enabled
    /// dictionaries, ordered by dictionary priority then import order,
    /// without glossaries, at most `max_hits` of them. Duplicate texts are
    /// fine. This is the query a scan issues for all of its candidate
    /// strings at once. `extra` is a disabled dictionary to search anyway
    /// (merge mode's main dictionary).
    pub fn find_terms(
        &self,
        texts: &[String],
        extra: Option<i64>,
        max_hits: usize,
    ) -> Result<Vec<TermHit>, StoreError> {
        let limit = i64::try_from(max_hits).unwrap_or(i64::MAX);
        let mut hits = Vec::new();
        for batch in texts.chunks(MAX_BATCH) {
            let mut stmt = self.conn.prepare(&find_terms_sql(batch.len()))?;
            let mut params: Vec<&dyn rusqlite::ToSql> =
                batch.iter().map(|t| t as &dyn rusqlite::ToSql).collect();
            params.push(&limit);
            params.push(&extra);
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
    /// At most `max_hits` rows are considered (a safety cap); the returned
    /// flag is true when more rows matched and some were left out.
    pub fn find_terms_bulk(
        &self,
        texts: &[String],
        extra: Option<i64>,
        max_hits: usize,
    ) -> Result<(Vec<BulkHit>, bool), StoreError> {
        let positions: HashMap<&str, usize> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| (t.as_str(), i))
            .collect();
        let mut hits = self.find_terms(texts, extra, max_hits.saturating_add(1))?;
        let truncated = hits.len() > max_hits;
        hits.truncate(max_hits);
        let mut claimed = Vec::new();
        for hit in hits {
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
        Ok((claimed, truncated))
    }

    /// Terms of the dictionaries with the given `(sequence, dictionary id)`
    /// pairs, enabled or not (Yomitan's findTermsBySequenceBulk); by pair
    /// position, then id. Pairs should be distinct.
    pub fn find_terms_by_sequence(
        &self,
        items: &[(i64, i64)],
    ) -> Result<Vec<(usize, TermHit)>, StoreError> {
        let mut result = Vec::new();
        let mut stmt = self.conn.prepare(&format!(
            "{HIT_COLUMNS} WHERE t.dictionary = ?1 AND t.sequence = ?2 ORDER BY t.id"
        ))?;
        for (index, &(sequence, dictionary)) in items.iter().enumerate() {
            let rows = stmt.query_map(rusqlite::params![dictionary, sequence], read_hit)?;
            for row in rows {
                result.push((index, row?));
            }
        }
        Ok(result)
    }

    /// Terms whose expression and reading equal one of the `(term, reading)`
    /// pairs, from `dictionary_ids` (Yomitan's findTermsExactBulk); by pair
    /// position, then id.
    pub fn find_terms_exact(
        &self,
        items: &[(String, String)],
        dictionary_ids: &[i64],
    ) -> Result<Vec<(usize, TermHit)>, StoreError> {
        let mut result = Vec::new();
        if dictionary_ids.is_empty() {
            return Ok(result);
        }
        let dictionaries = crate::metadata::placeholders(3, dictionary_ids.len());
        let mut stmt = self.conn.prepare(&format!(
            "{HIT_COLUMNS} WHERE t.expression = ?1 AND t.reading = ?2
                 AND +t.dictionary IN ({dictionaries})
             ORDER BY t.id"
        ))?;
        for (index, (term, reading)) in items.iter().enumerate() {
            let mut params: Vec<&dyn rusqlite::ToSql> = vec![term, reading];
            params.extend(dictionary_ids.iter().map(|d| d as &dyn rusqlite::ToSql));
            let rows = stmt.query_map(params.as_slice(), read_hit)?;
            for row in rows {
                result.push((index, row?));
            }
        }
        Ok(result)
    }

    /// SQLite's query plan (`EXPLAIN QUERY PLAN`) for [`Store::find_terms`]
    /// with `n` texts, one line per step. For benchmarks and diagnostics.
    pub fn explain_find_terms(&self, n: usize) -> Result<Vec<String>, StoreError> {
        let n = n.clamp(1, MAX_BATCH);
        let sql = format!("EXPLAIN QUERY PLAN {}", find_terms_sql(n));
        let mut stmt = self.conn.prepare(&sql)?;
        let texts: Vec<String> = (0..n).map(|i| format!("x{i}")).collect();
        let mut params: Vec<&dyn rusqlite::ToSql> =
            texts.iter().map(|t| t as &dyn rusqlite::ToSql).collect();
        params.push(&1i64);
        params.push(&rusqlite::types::Null);
        let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, String>(3))?;
        let mut lines = Vec::new();
        for row in rows {
            lines.push(row?);
        }
        Ok(lines)
    }
}

/// Columns read by [`read_hit`], with the dictionaries join.
const HIT_COLUMNS: &str = "SELECT t.id, t.dictionary, d.title, d.priority, t.expression, t.reading,
        t.definition_tags, t.rules, t.score, t.sequence, t.term_tags, t.form_of,
        t.content_count
 FROM terms t JOIN dictionaries d ON d.id = t.dictionary";

/// SQL matching `n` texts; the same placeholders (`?1..?n`) serve both the
/// expression and the reading lists, `?{n+1}` is the row limit and
/// `?{n+2}` an extra dictionary id (or NULL).
fn find_terms_sql(n: usize) -> String {
    let list = (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{HIT_COLUMNS}
         WHERE (d.enabled OR d.id = ?{extra})
             AND (t.expression IN ({list}) OR t.reading IN ({list}))
         ORDER BY d.priority, t.id
         LIMIT ?{limit}",
        limit = n + 1,
        extra = n + 2,
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
