// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kanji variant standardization (Yomitan's standardizeKanji preprocessor,
//! which calls convertVariants from the npm package kanji-processor 1.0.2,
//! MIT License, see NOTICE). The table holds exactly the variants that
//! package replaces: its regex has no `u` flag, so variants outside the
//! Basic Multilingual Plane never match and are not in the table.

use crate::data::KANJI_VARIANTS;

/// Replaces each kanji variant (e.g. a traditional form) with its standard
/// form; other characters are unchanged.
pub fn convert_variants(text: &str) -> String {
    text.chars()
        .map(|c| {
            KANJI_VARIANTS
                .binary_search_by_key(&c, |pair| pair.0)
                .ok()
                .and_then(|i| KANJI_VARIANTS.get(i))
                .map_or(c, |pair| pair.1)
        })
        .collect()
}
