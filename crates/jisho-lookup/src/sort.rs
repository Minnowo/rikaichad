// Ported from Yomitan ext/js/language/translator.js (_sortTermDictionaryEntries,
// _getShortestTextProcessingChainLength, _getShortestInflectionChainLength)
// at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Result ordering. Yomitan breaks the last tie between equal-length
//! headwords with Intl.Collator('en-US'); jisho compares code points
//! instead (docs/brief.md section 5).

use std::cmp::Ordering;

use crate::entry::TermEntry;
use crate::utf16::utf16_len;

/// Sorts entries by Yomitan's criteria, in order: primary-reading match,
/// longer source text, shorter text-processing chain, shorter inflection
/// chain, more exact source matches, frequency order, dictionary order,
/// higher score, longer then lexically smaller headword terms, more
/// definitions. Stable, like JavaScript's sort.
pub fn sort_term_entries(entries: &mut [TermEntry]) {
    entries.sort_by(compare);
}

fn compare(v1: &TermEntry, v2: &TermEntry) -> Ordering {
    v2.match_primary_reading
        .cmp(&v1.match_primary_reading)
        .then_with(|| {
            v2.max_original_text_length
                .cmp(&v1.max_original_text_length)
        })
        .then_with(|| {
            shortest_processing(&v1.text_processor_rule_chain_candidates).cmp(&shortest_processing(
                &v2.text_processor_rule_chain_candidates,
            ))
        })
        .then_with(|| shortest_inflection(v1).cmp(&shortest_inflection(v2)))
        .then_with(|| {
            v2.source_term_exact_match_count
                .cmp(&v1.source_term_exact_match_count)
        })
        .then_with(|| v1.frequency_order.cmp(&v2.frequency_order))
        .then_with(|| v1.dictionary_index.cmp(&v2.dictionary_index))
        .then_with(|| v2.score.partial_cmp(&v1.score).unwrap_or(Ordering::Equal))
        .then_with(|| compare_headwords(v1, v2))
        .then_with(|| v2.definitions.len().cmp(&v1.definitions.len()))
}

fn compare_headwords(v1: &TermEntry, v2: &TermEntry) -> Ordering {
    for (h1, h2) in v1.headwords.iter().zip(&v2.headwords) {
        let by_length = utf16_len(&h2.term).cmp(&utf16_len(&h1.term));
        if by_length != Ordering::Equal {
            return by_length;
        }
        let by_text = h1.term.cmp(&h2.term);
        if by_text != Ordering::Equal {
            return by_text;
        }
    }
    Ordering::Equal
}

fn shortest_processing(chains: &[Vec<String>]) -> usize {
    chains.iter().map(Vec::len).min().unwrap_or(0)
}

fn shortest_inflection(entry: &TermEntry) -> usize {
    entry
        .inflection_rule_chain_candidates
        .iter()
        .map(|c| c.inflection_rules.len())
        .min()
        .unwrap_or(0)
}
