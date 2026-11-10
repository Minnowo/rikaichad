// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON shape of term lookup results (`POST /v1/terms/lookup`, and the
//! CLI's `lookup --json`).

use jisho_format::glossary_item_text;
use jisho_store::TermRecord;
use serde::Serialize;
use serde_json::Value;

use crate::text_length::TextLength;

/// Response of a term lookup.
#[derive(Debug, Clone, Serialize)]
pub struct LookupResponse {
    pub entries: Vec<TermEntryJson>,
    pub matched: MatchedJson,
    /// True when entries were dropped by `limit` or the response size cap.
    pub truncated: bool,
}

/// The part of the input text that produced the results.
#[derive(Debug, Clone, Serialize)]
pub struct MatchedJson {
    pub text: String,
    pub length: TextLength,
}

/// One term entry.
#[derive(Debug, Clone, Serialize)]
pub struct TermEntryJson {
    pub dictionary: DictionaryRefJson,
    pub term: String,
    pub reading: String,
    pub definition_tags: Vec<String>,
    pub rules: Vec<String>,
    pub score: f64,
    pub sequence: i64,
    pub term_tags: Vec<String>,
    /// Glossary items as imported (raw JSON).
    pub glossary: Vec<Value>,
    /// One plain-text rendering per `glossary` item.
    pub glossary_text: Vec<String>,
}

/// Which dictionary an entry came from.
#[derive(Debug, Clone, Serialize)]
pub struct DictionaryRefJson {
    pub id: i64,
    pub title: String,
}

impl TermEntryJson {
    /// Converts a stored term into its JSON form, rendering glossary text.
    pub fn from_record(record: TermRecord) -> TermEntryJson {
        let glossary_text = record.glossary.iter().map(glossary_item_text).collect();
        let hit = record.hit;
        TermEntryJson {
            dictionary: DictionaryRefJson {
                id: hit.dictionary_id,
                title: hit.dictionary_title,
            },
            term: hit.term,
            reading: hit.reading,
            definition_tags: hit.definition_tags,
            rules: hit.rules,
            score: hit.score,
            sequence: hit.sequence,
            term_tags: hit.term_tags,
            glossary: record.glossary,
            glossary_text,
        }
    }
}

/// Builds a lookup response from `records` (fetched with `limit + 1` so
/// truncation by `limit` can be detected), keeping at most `limit` entries
/// and at most `max_bytes` of serialized entries.
pub fn lookup_response(
    matched_text: &str,
    records: Vec<TermRecord>,
    limit: usize,
    max_bytes: usize,
) -> LookupResponse {
    let mut truncated = records.len() > limit;
    let mut entries = Vec::new();
    let mut bytes = 0usize;
    for record in records.into_iter().take(limit) {
        let entry = TermEntryJson::from_record(record);
        let size = serde_json::to_vec(&entry).map(|v| v.len()).unwrap_or(0);
        if bytes.saturating_add(size) > max_bytes {
            truncated = true;
            break;
        }
        bytes += size;
        entries.push(entry);
    }
    LookupResponse {
        entries,
        matched: MatchedJson {
            text: matched_text.to_string(),
            length: TextLength::of(matched_text),
        },
        truncated,
    }
}
