// Ported from Yomitan ext/js/background/backend.js (_textParseScanning) at
// ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Sentence parsing with Yomitan's scanning parser: from each position,
//! the longest match of a lookup is one token (with furigana); characters
//! nothing matches are collected into tokens of their own.

use rikaichad_ja::{FuriganaSegment, distribute_furigana_inflected, is_code_point_japanese};
use rikaichad_store::{Store, StoreError};

use crate::find_terms::find_terms;
use crate::options::{EnabledDictionaries, FindTermsOptions, ResultMode};

/// One token of a parsed text.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedToken {
    /// The text split into pieces with readings (one piece without a
    /// reading for unmatched text).
    pub segments: Vec<FuriganaSegment>,
    /// The headwords whose lookup matched exactly this token, per entry in
    /// result order; empty for unmatched text.
    pub headwords: Vec<Vec<ParsedHeadword>>,
}

impl ParsedToken {
    /// The token's text.
    pub fn text(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }
}

/// A headword a token matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHeadword {
    pub term: String,
    pub reading: String,
}

/// _textParseScanning: parses `text`, looking up `scan_length` UTF-16 units
/// from each position (Yomitan's scanning.length), in "simple" mode.
pub fn parse_text(
    store: &Store,
    text: &str,
    dictionaries: &EnabledDictionaries,
    scan_length: usize,
) -> Result<Vec<ParsedToken>, StoreError> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let options = FindTermsOptions {
        mode: ResultMode::Simple,
        ..FindTermsOptions::default()
    };
    let mut tokens: Vec<ParsedToken> = Vec::new();
    // Whether the last token is unmatched text that grows.
    let mut ungrouped = false;
    let mut i = 0;
    while i < units.len() {
        let (code_point, character_length) = code_point_at(&units, i);
        let end = (i + scan_length).min(units.len());
        let substring = String::from_utf16_lossy(units.get(i..end).unwrap_or_default());
        let result = find_terms(store, &substring, dictionaries, &options)?;
        let matched_length = result.original_text_length;
        let mut token = None;
        if let Some(first) = result.entries.first()
            && matched_length > 0
            && (matched_length != character_length || is_code_point_japanese(code_point))
            && let Some(headword) = first.headwords.first()
        {
            let source_units = units
                .get(i..(i + matched_length).min(units.len()))
                .unwrap_or_default();
            let source = String::from_utf16_lossy(source_units);
            let segments =
                distribute_furigana_inflected(&headword.term, &headword.reading, &source);
            if !segments.is_empty() {
                let token_text: String = segments.iter().map(|s| s.text.as_str()).collect();
                let headwords = result
                    .entries
                    .iter()
                    .map(|entry| {
                        entry
                            .headwords
                            .iter()
                            .filter(|h| {
                                h.sources.iter().any(|s| {
                                    s.original_text == token_text
                                        && s.is_primary
                                        && s.match_type == "exact"
                                })
                            })
                            .map(|h| ParsedHeadword {
                                term: h.term.clone(),
                                reading: h.reading.clone(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .filter(|list| !list.is_empty())
                    .collect();
                token = Some(ParsedToken {
                    segments,
                    headwords,
                });
            }
        }
        match token {
            Some(token) => {
                ungrouped = false;
                tokens.push(token);
                i += matched_length;
            }
            None => {
                let character = String::from_utf16_lossy(
                    units.get(i..i + character_length).unwrap_or_default(),
                );
                match tokens.last_mut().and_then(|t| t.segments.first_mut()) {
                    Some(segment) if ungrouped => segment.text.push_str(&character),
                    _ => {
                        tokens.push(ParsedToken {
                            segments: vec![FuriganaSegment {
                                text: character,
                                reading: String::new(),
                            }],
                            headwords: Vec::new(),
                        });
                        ungrouped = true;
                    }
                }
                i += character_length;
            }
        }
    }
    Ok(tokens)
}

/// String.codePointAt at UTF-16 index `i`: the code point and its length.
fn code_point_at(units: &[u16], i: usize) -> (u32, usize) {
    match (units.get(i), units.get(i + 1)) {
        (Some(&high), Some(&low))
            if (0xd800..0xdc00).contains(&high) && (0xdc00..0xe000).contains(&low) =>
        {
            (
                0x10000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00),
                2,
            )
        }
        (Some(&unit), _) => (u32::from(unit), 1),
        (None, _) => (0, 1),
    }
}
