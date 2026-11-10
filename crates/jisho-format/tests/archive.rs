// SPDX-License-Identifier: AGPL-3.0-or-later
//! Archive reading at the crate's public interface, including hostile and
//! broken archives built byte by byte with jisho-testkit.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::io::Cursor;

use jisho_format::{
    Archive, EntryInfo, EntryKind, FormatError, Index, Limits, TermRow, stream_rows,
};
use jisho_testkit::{Method, ZipBuilder, fixture};

const INDEX: &str = r#"{"title":"T","revision":"1","format":3}"#;

#[test]
fn reads_fixture_entries_in_order_and_classifies_them() {
    let archive = open(fixture::terms_dictionary("T"));
    let kinds: Vec<EntryKind> = archive.entries().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [
            EntryKind::Index,
            EntryKind::TermBank,
            EntryKind::TermBank,
            EntryKind::TagBank,
            EntryKind::TermMetaBank,
            EntryKind::KanjiBank,
            EntryKind::KanjiMetaBank,
            EntryKind::Styles,
            EntryKind::Other,
        ]
    );
}

#[test]
fn parses_index_and_streams_term_rows() {
    let mut archive = open(fixture::terms_dictionary("Fixture"));
    let index_entry = archive.index_entry().unwrap();
    let (text, warning) = archive.read_to_string(&index_entry, 1 << 20).unwrap();
    assert!(warning.is_none());
    assert_eq!(Index::parse(&text).unwrap().title, "Fixture");

    let bank = archive.entries()[1].clone();
    let (rows, _) = archive
        .read_entry(&bank, 1 << 20, |reader| {
            let mut rows = Vec::new();
            stream_rows(
                reader,
                &bank.name,
                1 << 20,
                |row: TermRow| -> Result<(), FormatError> {
                    rows.push(row);
                    Ok(())
                },
            )?;
            Ok::<_, FormatError>(rows)
        })
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].reading, "たべる");
}

#[test]
fn bad_crc_is_a_warning_and_data_is_still_read() {
    let mut zip = ZipBuilder::new();
    zip.add("index.json", INDEX.as_bytes()).crc_override = Some(0xdead_beef);
    let mut archive = open(zip.finish());
    let entry = archive.index_entry().unwrap();
    let (text, warning) = archive.read_to_string(&entry, 1 << 20).unwrap();
    assert_eq!(text, INDEX);
    let warning = warning.unwrap();
    assert_eq!(warning.entry, "index.json");
    assert!(
        warning.message.contains("CRC-32 mismatch"),
        "{}",
        warning.message
    );
}

#[test]
fn stored_entries_are_read() {
    let mut zip = ZipBuilder::new();
    zip.add("index.json", INDEX.as_bytes()).method = Method::Stored;
    let mut archive = open(zip.finish());
    let entry = archive.index_entry().unwrap();
    assert_eq!(archive.read_to_string(&entry, 1 << 20).unwrap().0, INDEX);
}

#[test]
fn data_larger_than_declared_size_is_rejected() {
    // A zip bomb: the header claims 10 bytes, the stream inflates to 1 MiB.
    let mut zip = ZipBuilder::new();
    zip.add("term_bank_1.json", &vec![b' '; 1 << 20])
        .size_override = Some(10);
    let mut archive = open(zip.finish());
    let entry = archive.entries()[0].clone();
    let err = read_all(&mut archive, &entry).unwrap_err().to_string();
    assert!(err.contains("larger than its declared size"), "{err}");
}

#[test]
fn data_smaller_than_declared_size_is_rejected() {
    let mut zip = ZipBuilder::new();
    zip.add("term_bank_1.json", b"[]").size_override = Some(100);
    let mut archive = open(zip.finish());
    let entry = archive.entries()[0].clone();
    let err = read_all(&mut archive, &entry).unwrap_err().to_string();
    assert!(err.contains("smaller than its declared size"), "{err}");
}

#[test]
fn declared_size_over_cap_is_rejected_before_reading() {
    let mut zip = ZipBuilder::new();
    zip.add("term_bank_1.json", b"[]").size_override = Some(u32::MAX);
    let mut archive = open(zip.finish());
    let entry = archive.entries()[0].clone();
    let err = archive
        .read_entry(&entry, 1 << 20, |_| Ok::<(), FormatError>(()))
        .unwrap_err();
    assert!(matches!(err, FormatError::Limit { .. }), "{err}");
}

#[test]
fn encrypted_and_unsupported_methods_are_rejected() {
    let mut zip = ZipBuilder::new();
    zip.add("term_bank_1.json", b"[]").encrypted_flag = true;
    zip.add("term_bank_2.json", b"[]").method = Method::Raw(12); // bzip2
    let mut archive = open(zip.finish());
    let entries = archive.entries().to_vec();
    let err = read_all(&mut archive, &entries[0]).unwrap_err().to_string();
    assert!(err.contains("encrypted"), "{err}");
    let err = read_all(&mut archive, &entries[1]).unwrap_err().to_string();
    assert!(err.contains("unsupported compression method"), "{err}");
}

#[test]
fn too_many_entries_is_rejected() {
    let mut zip = ZipBuilder::new();
    for i in 0..5 {
        zip.add(&format!("img/{i}.png"), b"x");
    }
    let limits = Limits {
        max_entries: 4,
        ..Limits::default()
    };
    let err = Archive::from_reader(Cursor::new(zip.finish()), limits)
        .err()
        .unwrap();
    assert!(matches!(err, FormatError::Limit { .. }), "{err}");
}

#[test]
fn index_lookup_reports_missing_and_nested_index() {
    let mut zip = ZipBuilder::new();
    zip.add("term_bank_1.json", b"[]");
    assert!(matches!(
        open(zip.finish()).index_entry().unwrap_err(),
        FormatError::MissingIndex
    ));

    let mut zip = ZipBuilder::new();
    zip.add("__MACOSX/._index.json", b"junk");
    zip.add("MyDict/index.json", INDEX.as_bytes());
    let err = open(zip.finish()).index_entry().unwrap_err();
    assert!(
        matches!(&err, FormatError::NestedIndex { prefix } if prefix == "MyDict/"),
        "{err}"
    );
}

#[test]
fn traversal_names_are_never_dictionary_files() {
    let mut zip = ZipBuilder::new();
    zip.add("../index.json", INDEX.as_bytes());
    zip.add("/term_bank_1.json", b"[]");
    zip.add("..\\term_bank_2.json", b"[]");
    let archive = open(zip.finish());
    assert!(archive.entries().iter().all(|e| e.kind == EntryKind::Other));
}

#[test]
fn garbage_is_not_a_zip() {
    let err = Archive::from_reader(Cursor::new(b"not a zip".to_vec()), Limits::default())
        .err()
        .unwrap();
    assert!(matches!(err, FormatError::Zip { .. }), "{err}");
}

#[test]
fn deeply_nested_structured_content_is_rejected_not_overflowed() {
    let depth = 10_000;
    let content = format!(
        "{}\"x\"{}",
        "{\"tag\":\"div\",\"content\":".repeat(depth),
        "}".repeat(depth)
    );
    let bank = format!(
        r#"[["a","a","","",0,[{{"type":"structured-content","content":{content}}}],0,""]]"#
    );
    let mut reader: &[u8] = bank.as_bytes();
    let err = stream_rows(&mut reader, "term_bank_1.json", 1 << 30, |_row: TermRow| {
        Ok::<(), FormatError>(())
    })
    .unwrap_err()
    .to_string();
    assert!(err.contains("recursion limit"), "{err}");
}

#[test]
fn oversized_row_fails_while_reading_not_after() {
    let big = "x".repeat(4096);
    let bank = format!(r#"[["a","a","","",0,["ok"],0,""],["b","b","","",0,["{big}"],0,""]]"#);
    let mut reader: &[u8] = bank.as_bytes();
    let mut rows = 0;
    let err = stream_rows(&mut reader, "term_bank_1.json", 1024, |_row: TermRow| {
        rows += 1;
        Ok::<(), FormatError>(())
    })
    .unwrap_err()
    .to_string();
    assert_eq!(rows, 1, "the first row is still delivered");
    assert!(err.contains("row is larger than 1024 bytes"), "{err}");
}

#[test]
fn entry_count_is_checked_before_reading_the_central_directory() {
    // An end-of-central-directory record claiming 65535 entries with an
    // empty directory: the cap must trip from the record alone.
    let mut eocd = Vec::new();
    eocd.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    eocd.extend_from_slice(&[0, 0, 0, 0]);
    eocd.extend_from_slice(&0xffffu16.to_le_bytes());
    eocd.extend_from_slice(&0xffffu16.to_le_bytes());
    eocd.extend_from_slice(&0u32.to_le_bytes());
    eocd.extend_from_slice(&0u32.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes());
    let limits = Limits {
        max_entries: 1000,
        ..Limits::default()
    };
    let err = Archive::from_reader(Cursor::new(eocd), limits)
        .err()
        .unwrap();
    assert!(
        matches!(err, FormatError::Limit { actual: 65535, .. }),
        "{err}"
    );
}

fn open(bytes: Vec<u8>) -> Archive<Cursor<Vec<u8>>> {
    Archive::from_reader(Cursor::new(bytes), Limits::default()).unwrap()
}

fn read_all(
    archive: &mut Archive<Cursor<Vec<u8>>>,
    entry: &EntryInfo,
) -> Result<Vec<u8>, FormatError> {
    archive
        .read_entry(entry, 1 << 30, |reader| {
            let mut data = Vec::new();
            reader
                .read_to_end(&mut data)
                .map_err(|e| FormatError::Entry {
                    name: "test".to_string(),
                    message: e.to_string(),
                })?;
            Ok(data)
        })
        .map(|(data, _)| data)
}
