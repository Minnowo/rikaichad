// Ported from Yomitan ext/js/language/ja/japanese-text-preprocessors.js,
// the normalizeRadicalCharacters processor of ext/js/language/CJK-util.js
// and the Japanese entry of ext/js/language/language-descriptors.js at
// ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2024-2026  Yomitan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The text preprocessors Yomitan runs on Japanese lookup text, in the
//! order of its language descriptor. Each returns the input first, then
//! its variants; ids are the descriptor's keys and appear in the
//! textProcessorRuleChainCandidates of lookup results.

use crate::cjk::{normalize_cjk_compatibility_characters, normalize_radicals};
use crate::kana::{
    collapse_emphatic_sequences, convert_alphanumeric_to_full_width,
    convert_full_width_alphanumeric_to_normal, convert_half_width_kana_to_full_width,
    convert_hiragana_to_katakana, convert_katakana_to_hiragana, normalize_combining_characters,
};
use crate::kanji_variants::convert_variants;
use crate::romaji::convert_alphabetic_to_kana;

/// A named text preprocessor.
#[derive(Debug, Clone, Copy)]
pub struct TextPreprocessor {
    /// Identifier as in Yomitan (e.g. "convertHiraganaToKatakana").
    pub id: &'static str,
    /// Returns the input followed by its variants (duplicates allowed).
    pub process: fn(&str) -> Vec<String>,
}

/// Japanese preprocessors in Yomitan's order.
pub const JAPANESE_TEXT_PREPROCESSORS: [TextPreprocessor; 9] = [
    TextPreprocessor {
        id: "convertHalfWidthCharacters",
        process: |s| vec![s.to_string(), convert_half_width_kana_to_full_width(s)],
    },
    TextPreprocessor {
        id: "alphabeticToHiragana",
        process: |s| vec![s.to_string(), convert_alphabetic_to_kana(s)],
    },
    TextPreprocessor {
        id: "normalizeCombiningCharacters",
        process: |s| vec![s.to_string(), normalize_combining_characters(s)],
    },
    TextPreprocessor {
        id: "normalizeCJKCompatibilityCharacters",
        process: |s| vec![s.to_string(), normalize_cjk_compatibility_characters(s)],
    },
    TextPreprocessor {
        id: "normalizeRadicalCharacters",
        process: |s| vec![s.to_string(), normalize_radicals(s)],
    },
    TextPreprocessor {
        id: "alphanumericWidthVariants",
        process: |s| {
            vec![
                s.to_string(),
                convert_full_width_alphanumeric_to_normal(s),
                convert_alphanumeric_to_full_width(s),
            ]
        },
    },
    TextPreprocessor {
        id: "convertHiraganaToKatakana",
        process: |s| {
            vec![
                s.to_string(),
                convert_hiragana_to_katakana(s),
                convert_katakana_to_hiragana(s, false),
            ]
        },
    },
    TextPreprocessor {
        id: "collapseEmphaticSequences",
        process: |s| {
            vec![
                s.to_string(),
                collapse_emphatic_sequences(s, false),
                collapse_emphatic_sequences(s, true),
            ]
        },
    },
    TextPreprocessor {
        id: "standardizeKanji",
        process: |s| vec![s.to_string(), convert_variants(s)],
    },
];
