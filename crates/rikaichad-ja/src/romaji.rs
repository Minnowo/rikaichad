// Ported from Yomitan ext/js/language/ja/japanese-wanakana.js at
// ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2024-2026  Yomitan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Romaji -> hiragana conversion (Yomitan's alphabeticToHiragana
//! preprocessor). The replacement table is ROMAJI_TO_HIRAGANA from
//! japanese-kana-romaji-dicts.js, applied in its declaration order.

use crate::data::ROMAJI_TO_HIRAGANA;

const SMALL_TSU: char = '\u{3063}';

/// Converts runs of ASCII or fullwidth letters (and '-') in `text` to
/// hiragana, leaving every other character as it is.
pub fn convert_alphabetic_to_kana(text: &str) -> String {
    let mut part = String::new();
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        let code_point = u32::from(c);
        let normalized = match code_point {
            0x41..=0x5a => char::from_u32(code_point + (0x61 - 0x41)),
            0x61..=0x7a => Some(c),
            0xff21..=0xff3a => char::from_u32(code_point - (0xff21 - 0x61)),
            0xff41..=0xff5a => char::from_u32(code_point - (0xff41 - 0x61)),
            0x2d | 0xff0d => Some('-'),
            _ => None,
        };
        match normalized {
            Some(letter) => part.push(letter),
            None => {
                if !part.is_empty() {
                    result.push_str(&convert_to_hiragana(&part));
                    part.clear();
                }
                result.push(c);
            }
        }
    }
    if !part.is_empty() {
        result.push_str(&convert_to_hiragana(&part));
    }
    result
}

/// Lowercases `text` and applies every romaji replacement in table order,
/// then fills sokuon gaps (small tsu + letter before small tsu -> two small
/// tsu).
pub fn convert_to_hiragana(text: &str) -> String {
    let mut converted = text.to_lowercase();
    for (romaji, kana) in ROMAJI_TO_HIRAGANA {
        if converted.contains(romaji) {
            converted = converted.replace(romaji, kana);
        }
    }
    fill_sokuon_gaps(&converted)
}

/// `text.replaceAll(/\u3063[a-z](?=\u3063)/g, '\u3063\u3063')`:
/// non-overlapping matches left to right; the lookahead small tsu is not
/// consumed. (The katakana variant of
/// this step only matters for uppercase IME input, which rikaichad does not
/// convert.)
fn fill_sokuon_gaps(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        let next = chars.get(i + 1);
        let after = chars.get(i + 2);
        if c == SMALL_TSU && next.is_some_and(char::is_ascii_lowercase) && after == Some(&SMALL_TSU)
        {
            result.push(SMALL_TSU);
            result.push(SMALL_TSU);
            i += 2;
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}
