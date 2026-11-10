// SPDX-License-Identifier: AGPL-3.0-or-later
//! Store behaviour at its public interface: import, list, lookup, remove.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::time::{Duration, Instant};

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
    assert_eq!(store.check_integrity().unwrap(), Vec::<String>::new());
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
    assert_eq!(entry.hit.reading, "たべる");
    assert_eq!(entry.hit.definition_tags, ["v1", "vt"]);
    assert_eq!(entry.hit.rules, ["v1"]);
    assert_eq!(entry.hit.term_tags, ["P"]);
    assert_eq!(entry.hit.sequence, 100);
    // {type: "text"} glossary items are flattened at import, like Yomitan.
    assert_eq!(entry.glossary, [json!("to eat"), json!("to consume")]);

    let by_reading = store.lookup_exact("たべる", 10).unwrap();
    assert_eq!(by_reading, by_term);

    // Empty reading becomes the term; null definition tags become none.
    let katakana = store.lookup_exact("カタカナ", 10).unwrap();
    assert_eq!(katakana[0].hit.reading, "カタカナ");
    let rice = store.lookup_exact("ご飯", 10).unwrap();
    assert!(rice[0].hit.definition_tags.is_empty());

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
    let titles: Vec<&str> = all
        .iter()
        .map(|e| e.hit.dictionary_title.as_str())
        .collect();
    assert_eq!(titles, ["First", "Second"]);
    assert_eq!(store.lookup_exact("食べる", 1).unwrap().len(), 1);
}

#[test]
fn batched_find_then_glossaries_and_index_use() {
    let dir = TempDir::new("store-many");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    let texts: Vec<String> = ["食べる", "ごはん", "食べる", "無い"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let hits = store.find_terms(&texts, 100).unwrap();
    let terms: Vec<&str> = hits.iter().map(|r| r.term.as_str()).collect();
    assert_eq!(terms, ["食べる", "ご飯"]);
    assert!(store.find_terms(&[], 100).unwrap().is_empty());
    let records = store.with_glossaries(hits).unwrap();
    assert_eq!(records[1].glossary, [json!("cooked rice"), json!("meal")]);
    let plan = store.explain_find_terms(3).unwrap().join("\n");
    assert!(plan.contains("terms_expression"), "{plan}");
    assert!(plan.contains("terms_reading"), "{plan}");
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
    assert_eq!(store.check_integrity().unwrap(), Vec::<String>::new());
    let left = store.lookup_exact("食べる", 10).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].hit.dictionary_title, "B");
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

#[test]
fn concurrent_creation_of_a_fresh_store_succeeds() {
    for round in 0..5 {
        let dir = TempDir::new(&format!("store-race-{round}"));
        let path = dir.path().join("store.sqlite");
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || Store::open(&path).map(|_| ()))
            })
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
    }
}

#[test]
fn dictionary_ids_are_not_reused_after_removal() {
    let dir = TempDir::new("store-ids");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    let b = import(&mut store, &dir, "b.zip", fixture::terms_dictionary("B")).unwrap();
    store.remove(b.id).unwrap();
    let c = import(&mut store, &dir, "c.zip", fixture::terms_dictionary("C")).unwrap();
    assert!(c.id > b.id, "id {} reused (removed was {})", c.id, b.id);
}

#[test]
fn other_connections_read_during_an_import() {
    let dir = TempDir::new("store-during");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    let reader = open(&dir);
    let zip = dir.write("b.zip", &fixture::terms_dictionary("B"));
    let mut seen_during_import = Vec::new();
    store
        .import(&zip, &Limits::default(), &mut |_| {
            let started = Instant::now();
            let hits = reader.lookup_exact("食べる", 10).unwrap();
            seen_during_import.push((hits.len(), started.elapsed()));
        })
        .unwrap();
    assert!(!seen_during_import.is_empty());
    for (count, took) in seen_during_import {
        assert_eq!(count, 1, "only the committed dictionary is visible");
        assert!(took < Duration::from_secs(1), "reader blocked for {took:?}");
    }
    assert_eq!(reader.lookup_exact("食べる", 10).unwrap().len(), 2);
}

#[test]
fn expired_deadline_interrupts_queries() {
    let dir = TempDir::new("store-deadline");
    let mut store = open(&dir);
    // Enough matching rows that the query runs well past one deadline check.
    let rows: Vec<String> = (0..5000)
        .map(|i| format!(r#"["a","a","","",0,["gloss {i}"],{i},""]"#))
        .collect();
    let bank = format!("[{}]", rows.join(","));
    let zip = fixture::dictionary(&fixture::index("Many"), &[("term_bank_1.json", &bank)]);
    import(&mut store, &dir, "many.zip", zip).unwrap();
    store.set_deadline(Some(Instant::now())).unwrap();
    let err = store.lookup_exact("a", 5000).unwrap_err();
    assert!(matches!(err, StoreError::Interrupted), "{err}");
    store.set_deadline(None).unwrap();
    assert_eq!(store.lookup_exact("a", 5000).unwrap().len(), 5000);
}

#[test]
fn matching_is_capped_and_glossaries_respect_a_byte_budget() {
    let dir = TempDir::new("store-caps");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    import(&mut store, &dir, "b.zip", fixture::terms_dictionary("B")).unwrap();
    let texts = vec!["食べる".to_string(), "ご飯".to_string()];
    assert_eq!(store.find_terms(&texts, 10).unwrap().len(), 4);
    assert_eq!(store.find_terms(&texts, 3).unwrap().len(), 3);

    let hits = store.find_terms(&texts, 10).unwrap();
    // Each fixture glossary here is about 20-30 bytes of JSON.
    let (records, truncated) = store.with_glossaries_within(hits.clone(), 40).unwrap();
    assert_eq!(records.len(), 1);
    assert!(truncated);
    let (records, truncated) = store.with_glossaries_within(hits, 1 << 20).unwrap();
    assert_eq!(records.len(), 4);
    assert!(!truncated);
}

#[test]
fn oversized_row_is_rejected_at_import() {
    let dir = TempDir::new("store-bigrow");
    let mut store = open(&dir);
    let big = "x".repeat(2000);
    let zip = fixture::dictionary(
        &fixture::index("Big"),
        &[(
            "term_bank_1.json",
            &format!(r#"[["a","a","","",0,["{big}"],0,""]]"#),
        )],
    );
    let path = dir.write("big.zip", &zip);
    let limits = Limits {
        max_row_bytes: 1000,
        ..Limits::default()
    };
    let err = store.import(&path, &limits, &mut |_| {}).unwrap_err();
    assert!(
        err.to_string().contains("row is larger than 1000 bytes"),
        "{err}"
    );
    assert!(store.dictionaries().unwrap().is_empty());
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
