// SPDX-License-Identifier: AGPL-3.0-or-later
//! Records returned by store queries.

use jisho_format::Index;
use serde_json::Value;

/// Row counts of one dictionary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub terms: u64,
    pub term_meta: u64,
    pub kanji: u64,
    pub kanji_meta: u64,
    pub tags: u64,
}

/// An imported dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryInfo {
    pub id: i64,
    /// Position in the user's dictionary order; lower comes first.
    pub priority: i64,
    pub enabled: bool,
    /// Parsed `index.json` as imported.
    pub index: Index,
    pub has_styles: bool,
    /// File name of the zip it was imported from.
    pub source_name: String,
    /// SHA-256 of the zip it was imported from (hex).
    pub source_sha256: String,
    /// Import time, seconds since the Unix epoch.
    pub imported_at: i64,
    pub counts: Counts,
}

/// A matched term without its glossary: the small columns a scan ranks by.
#[derive(Debug, Clone, PartialEq)]
pub struct TermHit {
    /// Row id; follows import order.
    pub id: i64,
    pub dictionary_id: i64,
    pub dictionary_title: String,
    pub dictionary_priority: i64,
    pub term: String,
    /// Never empty: an empty reading was replaced by the term at import.
    pub reading: String,
    pub definition_tags: Vec<String>,
    pub rules: Vec<String>,
    pub score: f64,
    pub sequence: i64,
    pub term_tags: Vec<String>,
    /// Deinflection references from the glossary: (uninflected term, rule
    /// names). Empty for almost all entries.
    pub form_of: Vec<(String, Vec<String>)>,
    /// Number of glossary items that are not deinflection references.
    pub content_count: u32,
}

/// One term entry as stored (after Yomitan's import normalization).
#[derive(Debug, Clone, PartialEq)]
pub struct TermRecord {
    pub hit: TermHit,
    /// JSON array of glossary items.
    pub glossary: Vec<Value>,
}

/// Which index a bulk match came through (Yomitan's matchSource).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchSource {
    Term,
    Reading,
}

/// A term matched by [`crate::Store::find_terms_bulk`]: the hit, which input
/// text claimed it and through which index.
#[derive(Debug, Clone, PartialEq)]
pub struct BulkHit {
    pub hit: TermHit,
    /// Index into the input texts of the text that claimed this row.
    pub item_index: usize,
    pub match_source: MatchSource,
}

/// Splits a space-separated tag or rule list like Yomitan's _splitField:
/// an empty string gives no items; otherwise a plain split on ' ' (empty
/// items from double spaces are kept).
pub(crate) fn split_list(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(' ').map(str::to_string).collect()
}
