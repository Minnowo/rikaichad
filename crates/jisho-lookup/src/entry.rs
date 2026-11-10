// Ported from Yomitan ext/js/language/translator.js (the term and kanji
// dictionary entry structures it builds) at
// ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Lookup results, shaped like Yomitan's term and kanji dictionary entries.

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
    pub frequency_order: f64,
    pub dictionary_index: usize,
    /// The dictionary's display name; empty for grouped entries (Yomitan).
    pub dictionary_alias: String,
    pub source_term_exact_match_count: u32,
    pub match_primary_reading: bool,
    /// UTF-16 length of the longest source text.
    pub max_original_text_length: usize,
    pub headwords: Vec<Headword>,
    pub definitions: Vec<Definition>,
    pub pronunciations: Vec<TermPronunciation>,
    pub frequencies: Vec<TermFrequency>,
}

/// How an inflection chain was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// A tag, expanded from a dictionary's tag bank (Yomitan's Tag).
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub name: String,
    /// "default" when the dictionary gives none.
    pub category: String,
    pub order: f64,
    pub score: f64,
    /// The tag's notes, one per merged dictionary that has some.
    pub content: Vec<String>,
    /// Titles of the dictionaries the tag came from.
    pub dictionaries: Vec<String>,
    /// Set on a definition's part-of-speech tags that repeat the previous
    /// definition's (same dictionary).
    pub redundant: bool,
}

/// Tag names from one dictionary, before expansion (Yomitan's tag group).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TagGroup {
    pub dictionary_id: i64,
    pub dictionary: String,
    pub names: Vec<String>,
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
    /// Term tags, expanded (empty in "simple" mode, as in Yomitan).
    pub tags: Vec<Tag>,
    pub(crate) tag_groups: Vec<TagGroup>,
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
    /// "term", "reading" or "sequence": which index matched.
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
    pub frequency_order: f64,
    pub sequences: Vec<i64>,
    pub is_primary: bool,
    /// Definition tags, expanded (empty in "simple" mode).
    pub tags: Vec<Tag>,
    pub(crate) tag_groups: Vec<TagGroup>,
    /// Glossary items; empty until [`crate::load_glossaries`] fills them.
    /// Without deinflection-reference items, except for entries merge mode
    /// adds by sequence or secondary search (Yomitan keeps them there).
    pub entries: Vec<Value>,
    /// Keep deinflection-reference items when loading the glossary.
    pub(crate) raw_entries: bool,
    /// The row's glossary has deinflection references.
    pub(crate) has_form_of: bool,
}

/// Pitch accents or phonetic transcriptions of one headword from one
/// dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct TermPronunciation {
    pub index: usize,
    pub headword_index: usize,
    pub dictionary: String,
    pub dictionary_id: i64,
    pub dictionary_index: usize,
    pub dictionary_alias: String,
    pub pronunciations: Vec<Pronunciation>,
}

/// One pronunciation with its tags.
#[derive(Debug, Clone, PartialEq)]
pub struct Pronunciation {
    pub kind: PronunciationKind,
    /// Expanded tags (empty in "simple" mode).
    pub tags: Vec<Tag>,
    pub(crate) tag_groups: Vec<TagGroup>,
}

/// What a [`Pronunciation`] holds.
#[derive(Debug, Clone, PartialEq)]
pub enum PronunciationKind {
    PitchAccent {
        /// Downstep position (number) or an H/L pattern (string).
        positions: Value,
        nasal_positions: Vec<Value>,
        devoice_positions: Vec<Value>,
    },
    PhoneticTranscription {
        ipa: String,
    },
}

/// A frequency of one headword from one dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct TermFrequency {
    pub index: usize,
    pub headword_index: usize,
    pub dictionary: String,
    pub dictionary_id: i64,
    pub dictionary_index: usize,
    pub dictionary_alias: String,
    /// The data has a reading (the frequency is for that reading only).
    pub has_reading: bool,
    pub frequency: f64,
    pub display_value: Option<String>,
    /// `display_value` was the raw string and `frequency` parsed from it.
    pub display_value_parsed: bool,
    /// The dictionary's `frequencyMode` ("rank-based", "occurrence-based").
    pub frequency_mode: Option<String>,
}

/// A kanji result (Yomitan's KanjiDictionaryEntry).
#[derive(Debug, Clone, PartialEq)]
pub struct KanjiEntry {
    pub character: String,
    pub dictionary: String,
    pub dictionary_id: i64,
    pub dictionary_index: usize,
    pub dictionary_alias: String,
    pub onyomi: Vec<String>,
    pub kunyomi: Vec<String>,
    pub tags: Vec<Tag>,
    pub(crate) tag_groups: Vec<TagGroup>,
    /// Stats grouped by tag category, categories in first-seen order.
    pub stats: Vec<(String, Vec<KanjiStat>)>,
    pub definitions: Vec<String>,
    pub frequencies: Vec<KanjiFrequency>,
}

/// One kanji stat, described by the dictionary's tag of the same name.
#[derive(Debug, Clone, PartialEq)]
pub struct KanjiStat {
    pub name: String,
    pub category: String,
    /// The tag's notes.
    pub content: String,
    pub order: f64,
    pub score: f64,
    pub dictionary: String,
    pub value: String,
}

/// A frequency of a kanji from one dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct KanjiFrequency {
    pub index: usize,
    pub dictionary: String,
    pub dictionary_id: i64,
    pub dictionary_index: usize,
    pub dictionary_alias: String,
    pub character: String,
    pub frequency: f64,
    pub display_value: Option<String>,
    pub display_value_parsed: bool,
    pub frequency_mode: Option<String>,
}
