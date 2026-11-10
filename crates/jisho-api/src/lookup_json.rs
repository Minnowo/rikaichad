// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term lookup as used by `POST /v1/terms/lookup` and the CLI's `lookup`:
//! running the lookup ([`run_lookup`]) and its JSON shape.

use jisho_format::glossary_item_text;
use jisho_lookup::{EnabledDictionaries, FindTermsOptions, TermEntry, find_terms, load_glossaries};
use jisho_store::{Store, StoreError};
use serde::Serialize;
use serde_json::Value;

use crate::text_length::TextLength;

/// Bytes reserved for the response fields around `entries`.
pub const ENVELOPE_ALLOWANCE: usize = 1024;

/// How text is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupMode {
    /// Yomitan-style: the longest prefixes of the text, deinflected and
    /// normalized.
    Scan,
    /// Only entries whose term or reading is exactly the text (Yomitan with
    /// deinflection off).
    Exact,
}

/// What to look up and how much to return.
#[derive(Debug, Clone)]
pub struct LookupRequest {
    pub text: String,
    pub mode: LookupMode,
    /// At most this many entries.
    pub limit: usize,
    /// Entries with this reading sort first; empty for none.
    pub primary_reading: String,
    /// Cap on stored glossary bytes loaded for the response.
    pub max_bytes: usize,
}

/// Response of a term lookup.
#[derive(Debug, Clone, Serialize)]
pub struct LookupResponse {
    pub entries: Vec<TermEntryJson>,
    pub matched: MatchedJson,
    /// True when entries were dropped by `limit` or the size cap.
    pub truncated: bool,
}

/// The part of the input text that produced the results.
#[derive(Debug, Clone, Serialize)]
pub struct MatchedJson {
    pub text: String,
    pub length: TextLength,
}

/// One result: headwords, how they were reached, and their definitions.
#[derive(Debug, Clone, Serialize)]
pub struct TermEntryJson {
    pub headwords: Vec<HeadwordJson>,
    pub definitions: Vec<DefinitionJson>,
    /// Inflections undone, one chain per way the match was found; each
    /// chain starts at the inflection nearest the dictionary form (Yomitan's
    /// order), e.g. potential, negative, past for "tabe-rare-naka-tta".
    pub inflections: Vec<InflectionChainJson>,
    /// Text preprocessors applied (e.g. "convertHiraganaToKatakana"), one
    /// chain per way the match was found.
    pub text_processors: Vec<Vec<String>>,
    pub score: f64,
    pub dictionary_index: usize,
    pub is_primary: bool,
    /// UTF-16 length of the longest input prefix that reached this entry.
    pub max_original_text_length: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadwordJson {
    pub term: String,
    pub reading: String,
    pub sources: Vec<SourceJson>,
    pub term_tags: Vec<String>,
    /// Part-of-speech ids from the dictionary (e.g. "v1").
    pub word_classes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceJson {
    pub original_text: String,
    pub transformed_text: String,
    pub deinflected_text: String,
    pub match_type: &'static str,
    pub match_source: &'static str,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InflectionChainJson {
    /// "algorithm", "dictionary" or "both".
    pub source: &'static str,
    pub rules: Vec<RuleJson>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleJson {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DefinitionJson {
    pub dictionary: DictionaryRefJson,
    /// Store row id.
    pub id: i64,
    pub score: f64,
    pub sequences: Vec<i64>,
    pub definition_tags: Vec<String>,
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

/// Runs a lookup: finds and sorts entries, keeps at most `limit`, loads
/// glossaries within `max_bytes`. Blocking (store access).
pub fn run_lookup(store: &Store, request: &LookupRequest) -> Result<LookupResponse, StoreError> {
    let dictionaries = EnabledDictionaries::from_store(store)?;
    let options = FindTermsOptions {
        deinflect: request.mode == LookupMode::Scan,
        primary_reading: request.primary_reading.clone(),
        ..FindTermsOptions::default()
    };
    let mut result = find_terms(store, &request.text, &dictionaries, &options)?;
    let mut truncated = result.entries.len() > request.limit;
    result.entries.truncate(request.limit);
    let budget = request
        .max_bytes
        .saturating_sub(ENVELOPE_ALLOWANCE + request.text.len() * 2);
    let filled = load_glossaries(store, &mut result.entries, budget)?;
    if filled < result.entries.len() {
        truncated = true;
        result.entries.truncate(filled);
    }
    let matched = utf16_prefix(&request.text, result.original_text_length);
    Ok(LookupResponse {
        entries: result.entries.into_iter().map(entry_json).collect(),
        matched: MatchedJson {
            length: TextLength::of(matched),
            text: matched.to_string(),
        },
        truncated,
    })
}

fn entry_json(entry: TermEntry) -> TermEntryJson {
    TermEntryJson {
        headwords: entry
            .headwords
            .into_iter()
            .map(|h| HeadwordJson {
                term: h.term,
                reading: h.reading,
                sources: h
                    .sources
                    .into_iter()
                    .map(|s| SourceJson {
                        original_text: s.original_text,
                        transformed_text: s.transformed_text,
                        deinflected_text: s.deinflected_text,
                        match_type: s.match_type,
                        match_source: s.match_source,
                        is_primary: s.is_primary,
                    })
                    .collect(),
                term_tags: h.tags,
                word_classes: h.word_classes,
            })
            .collect(),
        definitions: entry
            .definitions
            .into_iter()
            .map(|d| DefinitionJson {
                dictionary: DictionaryRefJson {
                    id: d.dictionary_id,
                    title: d.dictionary,
                },
                id: d.id,
                score: d.score,
                sequences: d.sequences,
                definition_tags: d.tags,
                glossary_text: d.entries.iter().map(glossary_item_text).collect(),
                glossary: d.entries,
            })
            .collect(),
        inflections: entry
            .inflection_rule_chain_candidates
            .into_iter()
            .filter(|c| !c.inflection_rules.is_empty())
            .map(|c| InflectionChainJson {
                source: c.source.as_str(),
                rules: c
                    .inflection_rules
                    .into_iter()
                    .map(|r| RuleJson {
                        name: r.name,
                        description: r.description,
                    })
                    .collect(),
            })
            .collect(),
        text_processors: entry
            .text_processor_rule_chain_candidates
            .into_iter()
            .filter(|c| !c.is_empty())
            .collect(),
        score: entry.score,
        dictionary_index: entry.dictionary_index,
        is_primary: entry.is_primary,
        max_original_text_length: entry.max_original_text_length,
    }
}

/// The longest prefix of `text` that is at most `units` UTF-16 code units.
fn utf16_prefix(text: &str, units: usize) -> &str {
    let mut used = 0;
    for (i, c) in text.char_indices() {
        if used + c.len_utf16() > units {
            return text.get(..i).unwrap_or_default();
        }
        used += c.len_utf16();
    }
    text
}
