// Ported from Yomitan ext/js/language/translator.js (_sortTermDictionaryEntries,
// _sortTermDictionaryEntryDefinitions, _sortTermDictionaryEntrySimpleData,
// _getShortestTextProcessingChainLength, _getShortestInflectionChainLength)
// at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Result ordering. Yomitan breaks the last tie between equal-length
//! headwords with Intl.Collator('en-US'); see [`crate::collation`].

use std::cmp::Ordering;

use crate::collation;
use crate::entry::{Definition, TermEntry};
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
        .then_with(|| float_cmp(v1.frequency_order, v2.frequency_order))
        .then_with(|| v1.dictionary_index.cmp(&v2.dictionary_index))
        .then_with(|| float_cmp(v2.score, v1.score))
        .then_with(|| compare_headwords(v1, v2))
        .then_with(|| v2.definitions.len().cmp(&v1.definitions.len()))
}

fn compare_headwords(v1: &TermEntry, v2: &TermEntry) -> Ordering {
    for (h1, h2) in v1.headwords.iter().zip(&v2.headwords) {
        let by_length = utf16_len(&h2.term).cmp(&utf16_len(&h1.term));
        if by_length != Ordering::Equal {
            return by_length;
        }
        let by_text = collation::compare(&h1.term, &h2.term);
        if by_text != Ordering::Equal {
            return by_text;
        }
    }
    Ordering::Equal
}

/// _sortTermDictionaryEntryDefinitions: frequency order, dictionary order,
/// higher score, more headwords then smaller headword indices, original
/// order.
pub fn sort_definitions(definitions: &mut [Definition]) {
    if definitions.len() <= 1 {
        return;
    }
    definitions.sort_by(|v1, v2| {
        float_cmp(v1.frequency_order, v2.frequency_order)
            .then_with(|| v1.dictionary_index.cmp(&v2.dictionary_index))
            .then_with(|| float_cmp(v2.score, v1.score))
            .then_with(|| v2.headword_indices.len().cmp(&v1.headword_indices.len()))
            .then_with(|| v1.headword_indices.cmp(&v2.headword_indices))
            .then_with(|| v1.index.cmp(&v2.index))
    });
}

/// _sortTermDictionaryEntrySimpleData: by the key (headword index,
/// dictionary index, original index).
pub fn sort_simple_data<T>(items: &mut [T], key: impl Fn(&T) -> (usize, usize, usize)) {
    if items.len() > 1 {
        items.sort_by_key(key);
    }
}

/// `a - b` as an ordering, for finite numbers.
fn float_cmp(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
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
