// Ported from Yomitan ext/js/language/translator.js (_getAlgorithmDeinflections,
// _getNextSubstring, _createDeinflection) at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Candidate lookups for a text: every prefix (longest first), every text
//! variant of it, every deinflection of each variant.

use jisho_ja::{JAPANESE_TEXT_PREPROCESSORS, transform};
use jisho_store::BulkHit;

use crate::entry::{InflectionChain, InflectionSource, RuleName};
use crate::text_variants::{TextCache, text_variants};

/// One candidate: a deinflected text to look up, how it was derived, and
/// the dictionary rows matched to it.
#[derive(Debug, Clone)]
pub struct Deinflection {
    /// The input prefix.
    pub original_text: String,
    /// The prefix after text preprocessing.
    pub transformed_text: String,
    /// The text to look up.
    pub deinflected_text: String,
    /// Conditions the deinflected form carries (0: unconditioned).
    pub conditions: u32,
    pub text_processor_rule_chain_candidates: Vec<Vec<String>>,
    pub inflection_rule_chain_candidates: Vec<InflectionChain>,
    pub entries: Vec<BulkHit>,
}

/// All candidates for `text` (Yomitan's _getAlgorithmDeinflections with
/// searchResolution "letter" and no text replacements).
///
/// Yomitan shortens the text one UTF-16 code unit at a time; after an
/// astral character that briefly leaves a lone surrogate, a prefix that
/// can match nothing. Stepping by `char` gives the same candidates.
pub fn algorithm_deinflections(text: &str) -> Vec<Deinflection> {
    let mut deinflections = Vec::new();
    let mut cache = TextCache::default();
    let ends: Vec<usize> = text
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .rev()
        .collect();
    for end in ends {
        let Some(raw_source) = text.get(..end) else {
            continue;
        };
        for (source, chains) in text_variants(raw_source, &JAPANESE_TEXT_PREPROCESSORS, &mut cache)
        {
            let processor_chains: Vec<Vec<String>> = chains
                .iter()
                .map(|chain| chain.iter().map(|id| (*id).to_string()).collect())
                .collect();
            for candidate in transform(&source) {
                let rules = candidate
                    .trace
                    .iter()
                    .map(|frame| RuleName {
                        name: frame.transform.to_string(),
                        description: None,
                    })
                    .collect();
                deinflections.push(Deinflection {
                    original_text: raw_source.to_string(),
                    transformed_text: source.clone(),
                    deinflected_text: candidate.text,
                    conditions: candidate.conditions,
                    text_processor_rule_chain_candidates: processor_chains.clone(),
                    inflection_rule_chain_candidates: vec![InflectionChain {
                        source: InflectionSource::Algorithm,
                        inflection_rules: rules,
                    }],
                    entries: Vec::new(),
                });
            }
        }
    }
    deinflections
}

/// The single candidate used when deinflection is off.
pub fn identity_deinflection(text: &str) -> Deinflection {
    Deinflection {
        original_text: text.to_string(),
        transformed_text: text.to_string(),
        deinflected_text: text.to_string(),
        conditions: 0,
        text_processor_rule_chain_candidates: Vec::new(),
        inflection_rule_chain_candidates: Vec::new(),
        entries: Vec::new(),
    }
}
