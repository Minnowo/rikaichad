// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reading glossaries back: decompressed (see glossary_codec.rs) and
//! either kept as JSON text, to copy into responses, or parsed.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::StoreError;
use crate::glossary_codec::RawGlossary;
use crate::lookup::MAX_BATCH;
use crate::records::{TermHit, TermRecord};
use crate::store::Store;

/// A stored row: (term id, dictionary id, uncompressed size, payload).
type PayloadRow = (i64, i64, usize, Vec<u8>);

impl Store {
    /// The glossaries of the terms `ids` as stored: JSON text of the items
    /// (deinflection references included) and of their plain text.
    pub fn load_raw_glossaries(
        &self,
        ids: &[i64],
    ) -> Result<HashMap<i64, RawGlossary>, StoreError> {
        let mut glossaries = HashMap::with_capacity(ids.len());
        for batch in ids.chunks(MAX_BATCH) {
            let rows = self.payloads(batch)?;
            for (id, glossary) in self.decode(rows)? {
                glossaries.insert(id, glossary);
            }
        }
        Ok(glossaries)
    }

    /// The glossaries of the terms `ids`, parsed (all items, including
    /// deinflection references), by id.
    pub fn load_glossaries(&self, ids: &[i64]) -> Result<HashMap<i64, Vec<Value>>, StoreError> {
        self.load_raw_glossaries(ids)?
            .into_iter()
            .map(|(id, raw)| Ok((id, parse_items(id, &raw.json)?)))
            .collect()
    }

    /// The glossaries of the terms `ids`, in order, stopping before their
    /// total size (glossary and text JSON) would exceed `max_bytes`. Sizes
    /// are read before any glossary is, so an over-budget glossary never
    /// gets into memory. Returns the glossaries by id and whether any id
    /// was dropped.
    pub fn glossaries_within(
        &self,
        ids: &[i64],
        max_bytes: usize,
    ) -> Result<(HashMap<i64, RawGlossary>, bool), StoreError> {
        let sizes = self.glossary_sizes(ids)?;
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
        Ok((self.load_raw_glossaries(&kept)?, truncated))
    }

    /// Uncompressed glossary sizes (glossary and text JSON) of the terms
    /// `ids`, without reading the glossaries.
    pub fn glossary_sizes(&self, ids: &[i64]) -> Result<HashMap<i64, usize>, StoreError> {
        let mut sizes: HashMap<i64, usize> = HashMap::with_capacity(ids.len());
        for batch in ids.chunks(MAX_BATCH) {
            let placeholders = vec!["?"; batch.len()].join(", ");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT id, size FROM term_glossaries WHERE id IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(batch.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (id, size) = row?;
                // The payload's 4-byte length prefix is not response data.
                let size = usize::try_from(size).unwrap_or(usize::MAX);
                sizes.insert(id, size.saturating_sub(4));
            }
        }
        Ok(sizes)
    }

    /// Like [`Store::with_glossaries`], but stops before the total glossary
    /// size would exceed `max_bytes`. Returns the records and whether any
    /// hit was dropped.
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
                Some(raw) => {
                    let glossary = parse_items(hit.id, &raw.json)?;
                    records.push(TermRecord { hit, glossary });
                }
                None => break,
            }
        }
        Ok((records, truncated))
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

    /// A 64-bit hash of the glossary content of each of `items` (`(id,
    /// keep deinflection references)`), as Yomitan compares definitions in
    /// merge mode: the items, without deinflection references unless kept,
    /// serialized with integral numbers as integers. Rows are decompressed
    /// one at a time, so memory stays at one glossary.
    pub fn glossary_content_hashes(
        &self,
        items: &[(i64, bool)],
    ) -> Result<HashMap<i64, u64>, StoreError> {
        let mut hashes = HashMap::with_capacity(items.len());
        for &(id, keep_references) in items {
            let content = self.glossary_content(id, keep_references)?;
            let mut hasher = std::hash::DefaultHasher::new();
            std::hash::Hash::hash(&content, &mut hasher);
            hashes.insert(id, std::hash::Hasher::finish(&hasher));
        }
        Ok(hashes)
    }

    /// The compared content of one glossary (see
    /// [`Store::glossary_content_hashes`]), to confirm equal hashes.
    pub fn glossary_content(&self, id: i64, keep_references: bool) -> Result<String, StoreError> {
        let rows = self.payloads(&[id])?;
        let raw = self
            .decode(rows)?
            .pop()
            .map(|(_, raw)| raw)
            .ok_or_else(|| StoreError::NotFound {
                what: format!("glossary of term {id}"),
            })?;
        let mut items = parse_items(id, &raw.json)?;
        items.retain(|item| keep_references || !item.is_array());
        items
            .iter_mut()
            .for_each(rikaichad_format::canonicalize_numbers);
        serde_json::to_string(&items).map_err(|e| StoreError::Corrupt {
            message: format!("glossary of term {id}: {e}"),
        })
    }

    fn payloads(&self, ids: &[i64]) -> Result<Vec<PayloadRow>, StoreError> {
        let placeholders = vec!["?"; ids.len()].join(", ");
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, dictionary, size, payload FROM term_glossaries WHERE id IN ({placeholders})"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            let size: i64 = row.get(2)?;
            Ok((
                row.get(0)?,
                row.get(1)?,
                usize::try_from(size).unwrap_or(0),
                row.get(3)?,
            ))
        })?;
        let mut out = Vec::with_capacity(ids.len());
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Decompresses `rows`, loading the dictionaries they need into the
    /// connection's cache first.
    fn decode(&self, rows: Vec<PayloadRow>) -> Result<Vec<(i64, RawGlossary)>, StoreError> {
        let mut missing: Vec<i64> = Vec::new();
        {
            let cache = self.decoders.borrow();
            for (_, dictionary, _, _) in &rows {
                if !cache.contains(*dictionary) && !missing.contains(dictionary) {
                    missing.push(*dictionary);
                }
            }
        }
        if !missing.is_empty() {
            let mut cache = self.decoders.borrow_mut();
            let mut stmt = self
                .conn
                .prepare("SELECT trained FROM glossary_dictionaries WHERE dictionary = ?1")?;
            for dictionary in missing {
                let trained: Vec<u8> = stmt.query_row([dictionary], |row| row.get(0))?;
                cache.insert(dictionary, &trained);
            }
        }
        self.decoders.borrow().decode_all(rows)
    }
}

fn parse_items(id: i64, json: &str) -> Result<Vec<Value>, StoreError> {
    serde_json::from_str(json).map_err(|e| StoreError::Corrupt {
        message: format!("glossary of term {id}: {e}"),
    })
}
