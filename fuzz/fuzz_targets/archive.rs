// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arbitrary bytes as a dictionary zip: the central directory, then every
//! entry read and parsed as the importer does (index, banks, styles).
#![no_main]

use std::io::{Cursor, Read};

use jisho_format::{
    Archive, EntryKind, FormatError, Index, KanjiMetaRow, KanjiRow, Limits, TagRow, TermMetaRow,
    TermRow, normalize_glossary, stream_rows,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();
    let Ok(mut archive) = Archive::from_reader(Cursor::new(data), limits.clone()) else {
        return;
    };
    let entries = archive.entries().to_vec();
    if let Ok(index) = archive.index_entry()
        && let Ok((text, _)) = archive.read_to_string(&index, limits.max_index_bytes)
    {
        let _ = Index::parse(&text);
    }
    let max_row = limits.max_row_bytes;
    for entry in &entries {
        let _ = archive.read_entry(entry, limits.max_bank_bytes, |reader: &mut dyn Read| {
            parse_bank(entry.kind, reader, &entry.name, max_row)
        });
    }
});

fn parse_bank(
    kind: EntryKind,
    reader: &mut dyn Read,
    name: &str,
    max_row: u64,
) -> Result<u64, FormatError> {
    match kind {
        EntryKind::TermBank => stream_rows(reader, name, max_row, |mut row: TermRow| {
            let _ = normalize_glossary(&mut row.glossary);
            Ok::<(), FormatError>(())
        }),
        EntryKind::TermMetaBank => stream_rows(reader, name, max_row, |_: TermMetaRow| {
            Ok::<(), FormatError>(())
        }),
        EntryKind::KanjiBank => stream_rows(reader, name, max_row, |_: KanjiRow| {
            Ok::<(), FormatError>(())
        }),
        EntryKind::KanjiMetaBank => stream_rows(reader, name, max_row, |_: KanjiMetaRow| {
            Ok::<(), FormatError>(())
        }),
        EntryKind::TagBank => {
            stream_rows(reader, name, max_row, |_: TagRow| Ok::<(), FormatError>(()))
        }
        _ => {
            let mut sink = Vec::new();
            reader
                .take(1 << 20)
                .read_to_end(&mut sink)
                .map(|n| n as u64)
                .map_err(|e| FormatError::Entry {
                    name: name.to_string(),
                    message: e.to_string(),
                })
        }
    }
}
