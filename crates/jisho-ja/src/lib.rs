// SPDX-License-Identifier: AGPL-3.0-or-later
//! Japanese language support ported from Yomitan (see NOTICE for the
//! upstream files and commit): the deinflection transformer and its rule
//! table, the nine text preprocessors Yomitan applies to Japanese lookups,
//! and the kana, width, romaji, CJK and kanji-variant helpers behind them.
//!
//! All tables are compiled in (see build.rs); nothing here can fail at
//! runtime.

mod cjk;
mod cjk_languages;
mod data;
mod furigana;
mod kana;
mod kanji_variants;
mod romaji;
mod text_preprocessors;
mod transformer;

pub use cjk::{normalize_cjk_compatibility_characters, normalize_radicals};
pub use cjk_languages::{is_code_point_chinese, is_code_point_korean};
pub use furigana::{FuriganaSegment, distribute_furigana, distribute_furigana_inflected};
pub use kana::{
    collapse_emphatic_sequences, convert_alphanumeric_to_full_width,
    convert_full_width_alphanumeric_to_normal, convert_half_width_kana_to_full_width,
    convert_hiragana_to_katakana, convert_katakana_to_hiragana, is_code_point_japanese,
    is_code_point_kana, is_code_point_kanji, is_string_entirely_kana, is_string_partially_japanese,
    normalize_combining_characters,
};
pub use kanji_variants::convert_variants;
pub use romaji::{convert_alphabetic_to_kana, convert_to_hiragana};
pub use text_preprocessors::{JAPANESE_TEXT_PREPROCESSORS, TextPreprocessor};
pub use transformer::{
    TraceFrame, TransformedText, condition_flags_from_condition_type,
    condition_flags_from_parts_of_speech, conditions_match, transform, user_facing_rule,
};
