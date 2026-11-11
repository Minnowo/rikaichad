// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arbitrary bank JSON (first byte picks the bank kind) through the row
//! parsers; term glossaries are normalized and rendered as at import.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rikaichad_format::{
    FormatError, Index, KanjiMetaRow, KanjiRow, TagRow, TermMetaRow, TermRow, glossary_item_text,
    normalize_glossary, stream_rows,
};

const MAX_ROW: u64 = 1 << 20;

fuzz_target!(|data: &[u8]| {
    let Some((&kind, json)) = data.split_first() else {
        return;
    };
    let mut reader = json;
    let _ = match kind % 6 {
        0 => stream_rows(
            &mut reader,
            "term_bank_1.json",
            MAX_ROW,
            |mut row: TermRow| {
                if normalize_glossary(&mut row.glossary).is_ok() {
                    for item in &row.glossary {
                        let _ = glossary_item_text(item);
                    }
                }
                Ok::<(), FormatError>(())
            },
        ),
        1 => stream_rows(
            &mut reader,
            "term_meta_bank_1.json",
            MAX_ROW,
            |_: TermMetaRow| Ok::<(), FormatError>(()),
        ),
        2 => stream_rows(&mut reader, "kanji_bank_1.json", MAX_ROW, |_: KanjiRow| {
            Ok::<(), FormatError>(())
        }),
        3 => stream_rows(
            &mut reader,
            "kanji_meta_bank_1.json",
            MAX_ROW,
            |_: KanjiMetaRow| Ok::<(), FormatError>(()),
        ),
        4 => stream_rows(&mut reader, "tag_bank_1.json", MAX_ROW, |_: TagRow| {
            Ok::<(), FormatError>(())
        }),
        _ => {
            if let Ok(text) = std::str::from_utf8(json) {
                let _ = Index::parse(text);
            }
            Ok(0)
        }
    };
});
