// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term meta, kanji, kanji meta and tag queries, returning rows in the
//! order Yomitan's IndexedDB queries (`_findMultiBulk`, `_findFirstBulk`)
//! do: by request, then by id.

use std::collections::HashMap;

use rusqlite::types::ToSql;
use serde_json::Value;

use crate::error::StoreError;
use crate::records::split_list;
use crate::store::Store;

/// Keys per query; SQLite's default bound-parameter limit is 32766.
///
/// The queries write `+dictionary` so SQLite never picks the per-dictionary
/// index: with a few dozen keys it otherwise prefers it and scans every
/// row of the enabled dictionaries (240 ms instead of 0.2 ms, measured on
/// a store with 1.5M frequency rows).
const MAX_BATCH: usize = 4000;

/// A term meta row (frequency, pitch or IPA) matched by
/// [`Store::find_term_meta`].
#[derive(Debug, Clone, PartialEq)]
pub struct TermMetaHit {
    /// Position of the matched term in the request.
    pub index: usize,
    pub id: i64,
    pub dictionary_id: i64,
    pub term: String,
    /// "freq", "pitch" or "ipa".
    pub mode: String,
    pub data: Value,
}

/// A kanji row matched by [`Store::find_kanji`].
#[derive(Debug, Clone, PartialEq)]
pub struct KanjiHit {
    pub index: usize,
    pub id: i64,
    pub dictionary_id: i64,
    pub dictionary_title: String,
    pub character: String,
    pub onyomi: Vec<String>,
    pub kunyomi: Vec<String>,
    pub tags: Vec<String>,
    pub meanings: Vec<String>,
    /// Stat name and value, in source order (see jisho_format::KanjiStats).
    pub stats: Vec<(String, String)>,
}

/// A kanji meta row (frequency) matched by [`Store::find_kanji_meta`].
#[derive(Debug, Clone, PartialEq)]
pub struct KanjiMetaHit {
    pub index: usize,
    pub id: i64,
    pub dictionary_id: i64,
    pub character: String,
    pub mode: String,
    pub data: Value,
}

/// A tag definition from a dictionary's tag bank or index.json.
#[derive(Debug, Clone, PartialEq)]
pub struct TagRecord {
    pub category: String,
    pub order: f64,
    pub notes: String,
    pub score: f64,
}

impl Store {
    /// Term meta rows whose term is one of `terms` (distinct), from the
    /// dictionaries `dictionary_ids`; by term position, then id.
    pub fn find_term_meta(
        &self,
        terms: &[String],
        dictionary_ids: &[i64],
    ) -> Result<Vec<TermMetaHit>, StoreError> {
        let mut hits = self.by_keys(
            "SELECT id, dictionary, expression, mode, data FROM term_meta
             WHERE expression IN ({keys}) AND +dictionary IN ({dictionaries})",
            terms,
            dictionary_ids,
            |row| {
                let data: String = row.get(4)?;
                Ok(TermMetaHit {
                    index: 0,
                    id: row.get(0)?,
                    dictionary_id: row.get(1)?,
                    term: row.get(2)?,
                    mode: row.get(3)?,
                    data: parse_json(&data, 4)?,
                })
            },
        )?;
        let positions = positions(terms);
        for hit in &mut hits {
            hit.index = positions.get(hit.term.as_str()).copied().unwrap_or(0);
        }
        hits.sort_by_key(|h| (h.index, h.id));
        Ok(hits)
    }

    /// Kanji rows for `characters` (distinct) from `dictionary_ids`; by
    /// character position, then id.
    pub fn find_kanji(
        &self,
        characters: &[String],
        dictionary_ids: &[i64],
    ) -> Result<Vec<KanjiHit>, StoreError> {
        let mut hits = self.by_keys(
            "SELECT k.id, k.dictionary, d.title, k.character, k.onyomi, k.kunyomi, k.tags,
                    k.meanings, k.stats
             FROM kanji k JOIN dictionaries d ON d.id = k.dictionary
             WHERE k.character IN ({keys}) AND +k.dictionary IN ({dictionaries})",
            characters,
            dictionary_ids,
            |row| {
                let onyomi: String = row.get(4)?;
                let kunyomi: String = row.get(5)?;
                let tags: String = row.get(6)?;
                let meanings: String = row.get(7)?;
                let stats: String = row.get(8)?;
                Ok(KanjiHit {
                    index: 0,
                    id: row.get(0)?,
                    dictionary_id: row.get(1)?,
                    dictionary_title: row.get(2)?,
                    character: row.get(3)?,
                    onyomi: split_list(&onyomi),
                    kunyomi: split_list(&kunyomi),
                    tags: split_list(&tags),
                    meanings: serde_json::from_str(&meanings).map_err(|e| conversion(7, e))?,
                    stats: parse_stats(&stats).map_err(|e| conversion(8, e))?,
                })
            },
        )?;
        let positions = positions(characters);
        for hit in &mut hits {
            hit.index = positions.get(hit.character.as_str()).copied().unwrap_or(0);
        }
        hits.sort_by_key(|h| (h.index, h.id));
        Ok(hits)
    }

    /// Kanji meta rows for `characters` (distinct) from `dictionary_ids`;
    /// by character position, then id.
    pub fn find_kanji_meta(
        &self,
        characters: &[String],
        dictionary_ids: &[i64],
    ) -> Result<Vec<KanjiMetaHit>, StoreError> {
        let mut hits = self.by_keys(
            "SELECT id, dictionary, character, mode, data FROM kanji_meta
             WHERE character IN ({keys}) AND +dictionary IN ({dictionaries})",
            characters,
            dictionary_ids,
            |row| {
                let data: String = row.get(4)?;
                Ok(KanjiMetaHit {
                    index: 0,
                    id: row.get(0)?,
                    dictionary_id: row.get(1)?,
                    character: row.get(2)?,
                    mode: row.get(3)?,
                    data: parse_json(&data, 4)?,
                })
            },
        )?;
        let positions = positions(characters);
        for hit in &mut hits {
            hit.index = positions.get(hit.character.as_str()).copied().unwrap_or(0);
        }
        hits.sort_by_key(|h| (h.index, h.id));
        Ok(hits)
    }

    /// For each `(dictionary id, tag name)`, the first tag row (by id) with
    /// that name in that dictionary, if any (Yomitan's findTagMetaBulk).
    pub fn find_tags(&self, items: &[(i64, String)]) -> Result<Vec<Option<TagRecord>>, StoreError> {
        let mut names: Vec<String> = items.iter().map(|(_, n)| n.clone()).collect();
        names.sort();
        names.dedup();
        let mut dictionaries: Vec<i64> = items.iter().map(|(d, _)| *d).collect();
        dictionaries.sort_unstable();
        dictionaries.dedup();
        let rows = self.by_keys(
            "SELECT id, dictionary, name, category, sort_order, notes, score FROM tags
             WHERE name IN ({keys}) AND dictionary IN ({dictionaries})",
            &names,
            &dictionaries,
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    TagRecord {
                        category: row.get(3)?,
                        order: row.get(4)?,
                        notes: row.get(5)?,
                        score: row.get(6)?,
                    },
                ))
            },
        )?;
        let mut first: HashMap<(i64, String), (i64, TagRecord)> = HashMap::new();
        for (id, dictionary, name, tag) in rows {
            match first.get(&(dictionary, name.clone())) {
                Some((existing, _)) if *existing < id => {}
                _ => {
                    first.insert((dictionary, name), (id, tag));
                }
            }
        }
        Ok(items
            .iter()
            .map(|key| first.get(key).map(|(_, tag)| tag.clone()))
            .collect())
    }

    /// Runs `sql` with `{keys}` and `{dictionaries}` replaced by
    /// placeholder lists, in batches of keys.
    fn by_keys<T>(
        &self,
        sql: &str,
        keys: &[String],
        dictionary_ids: &[i64],
        mut map_row: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, StoreError> {
        let mut result = Vec::new();
        if keys.is_empty() || dictionary_ids.is_empty() {
            return Ok(result);
        }
        let batch_size = MAX_BATCH.saturating_sub(dictionary_ids.len()).max(1);
        for batch in keys.chunks(batch_size) {
            let key_list = placeholders(1, batch.len());
            let dictionary_list = placeholders(batch.len() + 1, dictionary_ids.len());
            let text = sql
                .replace("{keys}", &key_list)
                .replace("{dictionaries}", &dictionary_list);
            let mut stmt = self.conn.prepare(&text)?;
            let params: Vec<&dyn ToSql> = batch
                .iter()
                .map(|k| k as &dyn ToSql)
                .chain(dictionary_ids.iter().map(|d| d as &dyn ToSql))
                .collect();
            let rows = stmt.query_map(params.as_slice(), &mut map_row)?;
            for row in rows {
                result.push(row?);
            }
        }
        Ok(result)
    }
}

/// `?start, ?start+1, ...` (count of them).
pub(crate) fn placeholders(start: usize, count: usize) -> String {
    (start..start + count)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn positions(keys: &[String]) -> HashMap<&str, usize> {
    let mut map = HashMap::with_capacity(keys.len());
    for (i, key) in keys.iter().enumerate() {
        map.entry(key.as_str()).or_insert(i);
    }
    map
}

fn parse_json(text: &str, column: usize) -> rusqlite::Result<Value> {
    serde_json::from_str(text).map_err(|e| conversion(column, e))
}

/// Stats as stored: `[[name, value], ...]`, or an object in stores
/// imported before version 3.
fn parse_stats(text: &str) -> Result<Vec<(String, String)>, serde_json::Error> {
    if text.starts_with('[') {
        return serde_json::from_str(text);
    }
    let map: std::collections::BTreeMap<String, String> = serde_json::from_str(text)?;
    Ok(map.into_iter().collect())
}

fn conversion(column: usize, error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(error))
}
