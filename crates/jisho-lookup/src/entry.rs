// Ported from Yomitan ext/js/language/translator.js (the term dictionary
// entry structures it builds) at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Lookup results, shaped like Yomitan's term dictionary entries.

use serde_json::Value;

/// Result of [`crate::find_terms`].
#[derive(Debug, Clone, PartialEq)]
pub struct FindTermsResult {
    /// Sorted entries.
    pub entries: Vec<TermEntry>,
    /// Length of the longest matched prefix of the input, in UTF-16 units.
    pub original_text_length: usize,
    /// True when the matched-rows cap ([`crate::FindTermsOptions::max_hits`])
    /// left rows out, so results may be incomplete.
    pub truncated: bool,
}

/// One result: headwords with their definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct TermEntry {
    pub is_primary: bool,
    /// Text preprocessor ids applied, one chain per way the match was found.
    pub text_processor_rule_chain_candidates: Vec<Vec<String>>,
    /// Inflections undone, one chain per way the match was found.
    pub inflection_rule_chain_candidates: Vec<InflectionChain>,
    pub score: f64,
    pub frequency_order: i64,
    pub dictionary_index: usize,
    pub dictionary_alias: String,
    pub source_term_exact_match_count: u32,
    pub match_primary_reading: bool,
    /// UTF-16 length of the longest source text.
    pub max_original_text_length: usize,
    pub headwords: Vec<Headword>,
    pub definitions: Vec<Definition>,
}

/// How an inflection chain was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InflectionSource {
    Algorithm,
    Dictionary,
    Both,
}

impl InflectionSource {
    /// Yomitan's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            InflectionSource::Algorithm => "algorithm",
            InflectionSource::Dictionary => "dictionary",
            InflectionSource::Both => "both",
        }
    }
}

/// Inflections undone to reach the headword, the one nearest the
/// dictionary form first (Yomitan's trace order).
#[derive(Debug, Clone, PartialEq)]
pub struct InflectionChain {
    pub source: InflectionSource,
    pub inflection_rules: Vec<RuleName>,
}

/// A transform id, with its user-facing name and description once
/// results are finalized.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleName {
    pub name: String,
    pub description: Option<String>,
}

/// A headword (term + reading) and how it was reached from the input.
#[derive(Debug, Clone, PartialEq)]
pub struct Headword {
    pub index: usize,
    /// Position after grouping removes unused headwords (Yomitan sets it
    /// to `index` at creation).
    pub headword_index: usize,
    pub term: String,
    pub reading: String,
    pub sources: Vec<Source>,
    /// Term tags (expanded in milestone 3).
    pub tags: Vec<String>,
    /// Part-of-speech ids from the entry's `rules` field.
    pub word_classes: Vec<String>,
}

/// One path from the input text to a headword.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The input prefix.
    pub original_text: String,
    /// After text preprocessing.
    pub transformed_text: String,
    /// After deinflection (what was looked up).
    pub deinflected_text: String,
    /// Always "exact" in jisho v1.
    pub match_type: &'static str,
    /// "term" or "reading": which index matched.
    pub match_source: &'static str,
    pub is_primary: bool,
}

/// One dictionary definition of the headwords.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub index: usize,
    pub headword_indices: Vec<usize>,
    pub dictionary: String,
    pub dictionary_id: i64,
    pub dictionary_index: usize,
    pub dictionary_alias: String,
    /// Store row id.
    pub id: i64,
    pub score: f64,
    pub frequency_order: i64,
    pub sequences: Vec<i64>,
    pub is_primary: bool,
    /// Definition tags (expanded in milestone 3).
    pub tags: Vec<String>,
    /// Glossary items without deinflection references; empty until
    /// [`crate::load_glossaries`] fills them.
    pub entries: Vec<Value>,
}
