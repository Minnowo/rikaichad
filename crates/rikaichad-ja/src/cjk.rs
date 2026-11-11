// Ported from Yomitan ext/js/language/CJK-util.js and the
// normalizeCJKCompatibilityCharacters function of
// ext/js/language/ja/japanese.js at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2024-2026  Yomitan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! CJK code point ranges and the NFKD-based normalizations Yomitan applies
//! to CJK radicals and CJK compatibility characters. NFKD results come
//! from a table generated with String.prototype.normalize (data/nfkd.json),
//! so they match what Yomitan computes.

use crate::data::NFKD;

/// CJK unified ideographs (incl. extensions A-J) and compatibility
/// ideographs.
pub const CJK_IDEOGRAPH_RANGES: [(u32, u32); 13] = [
    (0x4e00, 0x9fff),
    (0x3400, 0x4dbf),
    (0x20000, 0x2a6df),
    (0x2a700, 0x2b73f),
    (0x2b740, 0x2b81f),
    (0x2b820, 0x2ceaf),
    (0x2ceb0, 0x2ebef),
    (0x30000, 0x3134f),
    (0x31350, 0x323af),
    (0x2ebf0, 0x2ee5f),
    (0x323b0, 0x3347f),
    (0xf900, 0xfaff),
    (0x2f800, 0x2fa1f),
];

/// Fullwidth digits, letters, punctuation and currency markers.
pub const FULLWIDTH_CHARACTER_RANGES: [(u32, u32); 8] = [
    (0xff10, 0xff19),
    (0xff21, 0xff3a),
    (0xff41, 0xff5a),
    (0xff01, 0xff0f),
    (0xff1a, 0xff1f),
    (0xff3b, 0xff3f),
    (0xff5b, 0xff60),
    (0xffe0, 0xffee),
];

/// CJK symbols and punctuation.
pub const CJK_PUNCTUATION_RANGE: (u32, u32) = (0x3000, 0x303f);

/// CJK compatibility (squared words such as U+3300).
const CJK_COMPATIBILITY: (u32, u32) = (0x3300, 0x33ff);

/// Kangxi radicals, CJK radicals supplement, CJK strokes.
const CJK_RADICALS_RANGES: [(u32, u32); 3] = [(0x2f00, 0x2fdf), (0x2e80, 0x2eff), (0x31c0, 0x31ef)];

/// Whether `code_point` is within the inclusive `range`.
pub fn in_range(code_point: u32, range: (u32, u32)) -> bool {
    code_point >= range.0 && code_point <= range.1
}

/// Whether `code_point` is within any of the inclusive `ranges`.
pub fn in_ranges(code_point: u32, ranges: &[(u32, u32)]) -> bool {
    ranges.iter().any(|range| in_range(code_point, *range))
}

/// NFKD-normalizes CJK radical and stroke characters, e.g. U+2F00 -> U+4E00.
pub fn normalize_radicals(text: &str) -> String {
    normalize_where(text, |c| in_ranges(c, &CJK_RADICALS_RANGES))
}

/// NFKD-normalizes CJK compatibility characters, e.g. U+3300 -> the
/// katakana it abbreviates.
pub fn normalize_cjk_compatibility_characters(text: &str) -> String {
    normalize_where(text, |c| in_range(c, CJK_COMPATIBILITY))
}

fn normalize_where(text: &str, selected: impl Fn(u32) -> bool) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        let code_point = u32::from(c);
        let replacement = if selected(code_point) {
            NFKD.binary_search_by_key(&code_point, |row| row.0)
                .ok()
                .and_then(|i| NFKD.get(i))
                .map(|row| row.1)
        } else {
            None
        };
        match replacement {
            Some(text) => result.push_str(text),
            None => result.push(c),
        }
    }
    result
}
