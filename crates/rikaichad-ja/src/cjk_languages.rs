// Ported from Yomitan ext/js/language/zh/chinese.js and
// ext/js/language/ko/korean.js at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2024-2026  Yomitan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Chinese and Korean character ranges, used by Yomitan's
//! removeNonJapaneseCharacters option (text is cut at the first character
//! that is neither Japanese, Chinese nor Korean).

use crate::cjk::{
    CJK_IDEOGRAPH_RANGES, CJK_PUNCTUATION_RANGE, FULLWIDTH_CHARACTER_RANGES, in_range, in_ranges,
};

const CHINESE_EXTRA_RANGES: [(u32, u32); 5] = [
    (0x3100, 0x312f),
    (0x31a0, 0x31bf),
    (0x16fe0, 0x16fff),
    (0xfe50, 0xfe6f),
    (0xfe10, 0xfe1f),
];

const KOREAN_EXTRA_RANGES: [(u32, u32); 6] = [
    (0x1100, 0x11ff),
    (0x3130, 0x318f),
    (0xac00, 0xd7af),
    (0xa960, 0xa97f),
    (0xd7b0, 0xd7ff),
    (0xffa0, 0xffdf),
];

/// Whether the code point is in Yomitan's Chinese ranges.
pub fn is_code_point_chinese(code_point: u32) -> bool {
    shared(code_point) || in_ranges(code_point, &CHINESE_EXTRA_RANGES)
}

/// Whether the code point is in Yomitan's Korean ranges.
pub fn is_code_point_korean(code_point: u32) -> bool {
    shared(code_point) || in_ranges(code_point, &KOREAN_EXTRA_RANGES)
}

fn shared(code_point: u32) -> bool {
    in_ranges(code_point, &CJK_IDEOGRAPH_RANGES)
        || in_range(code_point, CJK_PUNCTUATION_RANGE)
        || in_ranges(code_point, &FULLWIDTH_CHARACTER_RANGES)
}
