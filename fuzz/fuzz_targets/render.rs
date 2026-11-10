// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arbitrary JSON as a glossary item, rendered to plain text, before and
//! after import normalization (which also canonicalizes numbers).
#![no_main]

use jisho_format::{glossary_item_text, normalize_glossary};
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return;
    };
    let _ = glossary_item_text(&value);
    let mut items = vec![value];
    if normalize_glossary(&mut items).is_ok() {
        for item in &items {
            let _ = glossary_item_text(item);
        }
    }
});
