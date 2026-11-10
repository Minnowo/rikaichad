// SPDX-License-Identifier: AGPL-3.0-or-later
//! Store behaviour at its public interface: import, list, lookup, remove.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use jisho_format::Limits;
use jisho_store::{Counts, Store, StoreError};
use jisho_testkit::{TempDir, ZipBuilder, fixture};
use serde_json::json;

#[test]
fn imports_fixture_with_counts_styles_and_no_warnings() {
    let dir = TempDir::new("store-import");
    let mut store = open(&dir);
    let report = import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    assert_eq!(report.title, "A");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(
        report.counts,
        Counts {
            terms: 4,
            term_meta: 2,
            kanji: 1,
            kanji_meta: 1,
            tags: 2
        }
    );
    let dicts = store.dictionaries().unwrap();
    assert_eq!(dicts.len(), 1);
    assert_eq!(dicts[0].index.title, "A");
    assert!(dicts[0].has_styles);
    assert!(dicts[0].enabled);
    assert_eq!(dicts[0].source_name, "a.zip");
    assert_eq!(dicts[0].source_sha256.len(), 64);
    assert_eq!(dicts[0].counts, report.counts);
}

#[test]
fn exact_lookup_by_term_or_reading_with_import_normalization() {
    let dir = TempDir::new("store-lookup");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();

    let by_term = store.lookup_exact("食べる", 10).unwrap();
    assert_eq!(by_term.len(), 1);
    let entry = &by_term[0];
    assert_eq!(entry.reading, "たべる");
    assert_eq!(entry.definition_tags, ["v1", "vt"]);
    assert_eq!(entry.rules, ["v1"]);
    assert_eq!(entry.term_tags, ["P"]);
    assert_eq!(entry.sequence, 100);
    // {type: "text"} glossary items are flattened at import, like Yomitan.
    assert_eq!(entry.glossary, [json!("to eat"), json!("to consume")]);

    let by_reading = store.lookup_exact("たべる", 10).unwrap();
    assert_eq!(by_reading, by_term);

    // Empty reading becomes the term; null definition tags become none.
    let katakana = store.lookup_exact("カタカナ", 10).unwrap();
    assert_eq!(katakana[0].reading, "カタカナ");
    let rice = store.lookup_exact("ご飯", 10).unwrap();
    assert!(rice[0].definition_tags.is_empty());

    assert!(store.lookup_exact("無い", 10).unwrap().is_empty());
}

#[test]
fn results_follow_dictionary_order_and_respect_limit() {
    let dir = TempDir::new("store-order");
    let mut store = open(&dir);
    import(
        &mut store,
        &dir,
        "a.zip",
        fixture::terms_dictionary("First"),
    )
    .unwrap();
    import(
        &mut store,
        &dir,
        "b.zip",
        fixture::terms_dictionary("Second"),
    )
    .unwrap();
    let all = store.lookup_exact("食べる", 10).unwrap();
    let titles: Vec<&str> = all.iter().map(|e| e.dictionary_title.as_str()).collect();
    assert_eq!(titles, ["First", "Second"]);
    assert_eq!(store.lookup_exact("食べる", 1).unwrap().len(), 1);
}

#[test]
fn duplicate_title_is_rejected() {
    let dir = TempDir::new("store-dup");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    let err = import(&mut store, &dir, "a2.zip", fixture::terms_dictionary("A")).unwrap_err();
    assert!(
        matches!(err, StoreError::Conflict { ref title } if title == "A"),
        "{err}"
    );
    assert_eq!(store.dictionaries().unwrap().len(), 1);
}

#[test]
fn invalid_row_rolls_back_everything() {
    let dir = TempDir::new("store-rollback");
    let mut store = open(&dir);
    let zip = fixture::dictionary(
        &fixture::index("Broken"),
        &[
            ("term_bank_1.json", r#"[["ok","ok","","",0,["fine"],1,""]]"#),
            (
                "term_bank_2.json",
                r#"[["ok2","ok2","","",0,["fine"],2,""],["bad","bad","","",0,[{"type":"video"}],3,""]]"#,
            ),
        ],
    );
    let err = import(&mut store, &dir, "broken.zip", zip).unwrap_err();
    match &err {
        StoreError::InvalidRow {
            entry,
            row,
            message,
        } => {
            assert_eq!(entry, "term_bank_2.json");
            assert_eq!(*row, 1);
            assert!(message.contains("unknown type"), "{message}");
        }
        other => panic!("unexpected error: {other}"),
    }
    assert!(store.dictionaries().unwrap().is_empty());
    assert!(store.lookup_exact("ok", 10).unwrap().is_empty());
}

#[test]
fn bad_crc_imports_with_warning() {
    let dir = TempDir::new("store-crc");
    let mut store = open(&dir);
    let mut zip = ZipBuilder::new();
    zip.add("index.json", fixture::index("Crc").as_bytes());
    zip.add("term_meta_bank_1.json", br#"[["x","freq",1]]"#)
        .crc_override = Some(1);
    let report = import(&mut store, &dir, "crc.zip", zip.finish()).unwrap();
    assert_eq!(report.counts.term_meta, 1);
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.warnings[0].entry, "term_meta_bank_1.json");
}

#[test]
fn remove_deletes_all_rows_and_unknown_id_is_not_found() {
    let dir = TempDir::new("store-remove");
    let mut store = open(&dir);
    let a = import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    import(&mut store, &dir, "b.zip", fixture::terms_dictionary("B")).unwrap();
    store.remove(a.id).unwrap();
    let left = store.lookup_exact("食べる", 10).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].dictionary_title, "B");
    assert!(matches!(
        store.remove(a.id).unwrap_err(),
        StoreError::NotFound { .. }
    ));
    // The title is free again.
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
}

#[test]
fn store_persists_across_reopen() {
    let dir = TempDir::new("store-reopen");
    {
        let mut store = open(&dir);
        import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    }
    let store = open(&dir);
    assert_eq!(store.dictionaries().unwrap().len(), 1);
    assert_eq!(store.lookup_exact("ご飯", 10).unwrap().len(), 1);
}

#[test]
fn query_only_store_refuses_import_but_reads() {
    let dir = TempDir::new("store-ro");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    store.set_query_only().unwrap();
    let err = import(&mut store, &dir, "b.zip", fixture::terms_dictionary("B")).unwrap_err();
    assert!(matches!(err, StoreError::ReadOnly), "{err}");
    assert_eq!(store.lookup_exact("食べる", 10).unwrap().len(), 1);
}

#[test]
fn second_connection_reads_while_first_holds_store() {
    let dir = TempDir::new("store-two");
    let mut writer = open(&dir);
    import(&mut writer, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    let reader = open(&dir);
    assert_eq!(reader.lookup_exact("食べる", 10).unwrap().len(), 1);
}

fn open(dir: &TempDir) -> Store {
    Store::open(&dir.path().join("store.sqlite")).unwrap()
}

fn import(
    store: &mut Store,
    dir: &TempDir,
    name: &str,
    zip: Vec<u8>,
) -> Result<jisho_store::ImportReport, StoreError> {
    let path = dir.write(name, &zip);
    store.import(&path, &Limits::default(), &mut |_| {})
}
