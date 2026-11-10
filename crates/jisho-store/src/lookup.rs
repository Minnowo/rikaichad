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
    /// memory (`octet_length` reads the size from the record header without
    /// reading the text). Returns the glossaries by id and whether any id was dropped.
    pub fn glossaries_within(
        &self,
        ids: &[i64],
        max_bytes: usize,
    ) -> Result<(HashMap<i64, Vec<Value>>, bool), StoreError> {
        let mut sizes: HashMap<i64, usize> = HashMap::with_capacity(ids.len());
        for batch in ids.chunks(MAX_BATCH) {
            let placeholders = vec!["?"; batch.len()].join(", ");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT id, octet_length(glossary) FROM term_glossaries
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

    /// The stored glossaries of the terms `ids` (all items, including
    /// deinflection references), by id.
    pub fn load_glossaries(&self, ids: &[i64]) -> Result<HashMap<i64, Vec<Value>>, StoreError> {
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

    /// Stored glossary sizes in bytes of the terms `ids`, read from the
    /// record headers (`octet_length`), not from the glossaries.
    pub fn glossary_sizes(&self, ids: &[i64]) -> Result<HashMap<i64, usize>, StoreError> {
        let mut sizes: HashMap<i64, usize> = HashMap::with_capacity(ids.len());
        for batch in ids.chunks(MAX_BATCH) {
            let placeholders = vec!["?"; batch.len()].join(", ");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT id, octet_length(glossary) FROM term_glossaries
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
        Ok(sizes)
    }

    /// A 64-bit hash of the glossary content of each of `items` (`(id,
    /// keep deinflection references)`), as Yomitan compares definitions in
    /// merge mode: the items, without deinflection references unless kept,
    /// serialized with integral numbers as integers. Rows are read one at a
    /// time, so memory stays at one glossary.
    pub fn glossary_content_hashes(
        &self,
        items: &[(i64, bool)],
    ) -> Result<HashMap<i64, u64>, StoreError> {
        let keep: HashMap<i64, bool> = items.iter().copied().collect();
        let mut hashes = HashMap::with_capacity(items.len());
        let ids: Vec<i64> = items.iter().map(|(id, _)| *id).collect();
        for batch in ids.chunks(MAX_BATCH) {
            let placeholders = vec!["?"; batch.len()].join(", ");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT id, glossary FROM term_glossaries WHERE id IN ({placeholders})"
            ))?;
            let mut rows = stmt.query(rusqlite::params_from_iter(batch.iter()))?;
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                let text: String = row.get(1)?;
                let content = content_text(&text, keep.get(&id).copied().unwrap_or(false))?;
                let mut hasher = std::hash::DefaultHasher::new();
                std::hash::Hash::hash(&content, &mut hasher);
                hashes.insert(id, std::hash::Hasher::finish(&hasher));
            }
        }
        Ok(hashes)
    }

    /// The compared content of one glossary (see
    /// [`Store::glossary_content_hashes`]), to confirm equal hashes.
    pub fn glossary_content(&self, id: i64, keep_references: bool) -> Result<String, StoreError> {
        let text: String = self.conn.query_row(
            "SELECT glossary FROM term_glossaries WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        content_text(&text, keep_references)
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

/// The glossary `text` as merge mode compares it.
fn content_text(text: &str, keep_references: bool) -> Result<String, StoreError> {
    let conversion = |e: serde_json::Error| {
        StoreError::Sqlite(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(e),
        ))
    };
    let mut items: Vec<Value> = serde_json::from_str(text).map_err(conversion)?;
    items.retain(|item| keep_references || !item.is_array());
    items
        .iter_mut()
        .for_each(jisho_format::canonicalize_numbers);
    serde_json::to_string(&items).map_err(conversion)
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
