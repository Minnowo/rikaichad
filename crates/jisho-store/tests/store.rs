// SPDX-License-Identifier: AGPL-3.0-or-later
//! Store behaviour at its public interface: import, list, lookup, remove.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::time::{Duration, Instant};

use jisho_format::Limits;
use jisho_store::{
    Counts, DictionarySettings, FrequencyOrder, LookupSettings, MatchSource, Store, StoreError,
};
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
            tags: 3
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
    let hits = store.find_terms(&texts, None, 100).unwrap();
    let terms: Vec<&str> = hits.iter().map(|r| r.term.as_str()).collect();
    assert_eq!(terms, ["食べる", "ご飯"]);
    assert!(store.find_terms(&[], None, 100).unwrap().is_empty());
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
    assert_eq!(store.find_terms(&texts, None, 10).unwrap().len(), 4);
    assert_eq!(store.find_terms(&texts, None, 3).unwrap().len(), 3);

    let hits = store.find_terms(&texts, None, 10).unwrap();
    // Each fixture glossary here is about 40-60 bytes of JSON with its text.
    let (records, truncated) = store.with_glossaries_within(hits.clone(), 80).unwrap();
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

#[test]
fn deinflection_references_are_kept_out_of_glossary_content() {
    let dir = TempDir::new("store-formof");
    let mut store = open(&dir);
    let zip = fixture::dictionary(
        &fixture::index("Refs"),
        &[(
            "term_bank_1.json",
            r#"[["食べた","たべた","","",0,[["食べる",["past"]]],1,""],
                ["見た","みた","","",0,["saw",["見る",["past"]]],2,""]]"#,
        )],
    );
    import(&mut store, &dir, "refs.zip", zip).unwrap();
    let hits = store
        .find_terms(&["食べた".to_string(), "見た".to_string()], None, 10)
        .unwrap();
    assert_eq!(
        hits[0].form_of,
        [("食べる".to_string(), vec!["past".to_string()])]
    );
    assert_eq!(hits[0].content_count, 0);
    assert_eq!(hits[1].content_count, 1);
    assert_eq!(hits[1].form_of.len(), 1);
}

#[test]
fn bulk_lookup_claims_rows_like_indexeddb() {
    let dir = TempDir::new("store-bulk");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    // The "taberu" row matches text 0 by reading and text 1 by term: the
    // earlier text claims it, through the reading index.
    let texts = vec![
        "たべる".to_string(),
        "食べる".to_string(),
        "ご飯".to_string(),
    ];
    let (bulk, truncated) = store.find_terms_bulk(&texts, None, 100).unwrap();
    assert!(!truncated);
    let (capped, truncated) = store.find_terms_bulk(&texts, None, 1).unwrap();
    assert_eq!(capped.len(), 1);
    assert!(truncated, "hitting the cap is reported");
    let claims: Vec<(&str, usize, MatchSource)> = bulk
        .iter()
        .map(|b| (b.hit.term.as_str(), b.item_index, b.match_source))
        .collect();
    assert_eq!(
        claims,
        [
            ("食べる", 0, MatchSource::Reading),
            ("ご飯", 2, MatchSource::Term),
        ]
    );
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

#[test]
fn store_from_another_schema_version_is_refused_with_advice() {
    let dir = TempDir::new("store-version");
    let path = dir.path().join("store.sqlite");
    drop(Store::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "UPDATE meta SET value = '3' WHERE key = 'store_version'",
        [],
    )
    .unwrap();
    drop(conn);
    let err = Store::open(&path).err().unwrap();
    assert!(
        matches!(err, StoreError::UnsupportedStoreVersion { found: 3, .. }),
        "{err}"
    );
    assert!(
        err.to_string().contains("import your dictionaries again"),
        "{err}"
    );
}

#[test]
fn glossaries_are_stored_compressed_and_read_back() {
    let dir = TempDir::new("store-compressed");
    let mut store = open(&dir);
    import(&mut store, &dir, "a.zip", fixture::terms_dictionary("A")).unwrap();
    let hits = store.find_terms(&["ご飯".to_string()], None, 10).unwrap();
    let raw = store.load_raw_glossaries(&[hits[0].id]).unwrap();
    let raw = &raw[&hits[0].id];
    assert_eq!(raw.json, r#"["cooked rice","meal"]"#);
    assert_eq!(raw.text, r#"["cooked rice","meal"]"#);
    let sizes = store.glossary_sizes(&[hits[0].id]).unwrap();
    assert_eq!(sizes[&hits[0].id], raw.len());
    drop(store);
    // A second connection loads the dictionary's decoder itself.
    let store = open(&dir);
    let records = store.with_glossaries(hits).unwrap();
    assert_eq!(
        records[0].glossary,
        vec![json!("cooked rice"), json!("meal")]
    );
}

#[test]
fn settings_order_and_enable_flags_persist() {
    let dir = TempDir::new("store-settings");
    let mut store = open(&dir);
    for title in ["A", "B", "C"] {
        import(
            &mut store,
            &dir,
            &format!("{title}.zip"),
            fixture::terms_dictionary(title),
        )
        .unwrap();
    }
    let ids: Vec<i64> = store
        .lookup_dictionaries()
        .unwrap()
        .iter()
        .map(|d| d.id)
        .collect();
    // The first sequenced dictionary imported becomes the main one.
    assert_eq!(
        store.lookup_settings().unwrap().main_dictionary.as_deref(),
        Some("A")
    );

    store.set_order(&[ids[2], ids[0]]).unwrap();
    store.set_enabled(ids[1], false).unwrap();
    let settings = DictionarySettings {
        alias: Some("Cee".to_string()),
        allow_secondary_searches: true,
        parts_of_speech_filter: false,
        use_deinflections: false,
    };
    store.set_dictionary_settings(ids[2], &settings).unwrap();
    store
        .set_lookup_settings(&LookupSettings {
            main_dictionary: Some("C".to_string()),
            sort_frequency_dictionary: Some("B".to_string()),
            sort_frequency_order: FrequencyOrder::Ascending,
        })
        .unwrap();
    drop(store);

    let mut store = open(&dir);
    let dictionaries = store.lookup_dictionaries().unwrap();
    let titles: Vec<&str> = dictionaries.iter().map(|d| d.title.as_str()).collect();
    assert_eq!(titles, ["C", "A", "B"]);
    assert!(!dictionaries[2].enabled);
    assert_eq!(dictionaries[0].settings, settings);
    let lookup = store.lookup_settings().unwrap();
    assert_eq!(lookup.sort_frequency_order, FrequencyOrder::Ascending);
    // Disabled dictionaries are not searched.
    let hits = store.find_terms(&["食べる".to_string()], None, 10).unwrap();
    assert_eq!(hits.len(), 2);
    // ... unless named as the extra one (merge mode's main dictionary).
    let hits = store
        .find_terms(&["食べる".to_string()], Some(ids[1]), 10)
        .unwrap();
    assert_eq!(hits.len(), 3);

    assert!(matches!(
        store.set_order(&[ids[0], ids[0]]),
        Err(StoreError::Invalid { .. })
    ));
    assert!(matches!(
        store.set_order(&[999]),
        Err(StoreError::NotFound { .. })
    ));
    assert!(matches!(
        store.set_enabled(999, true),
        Err(StoreError::NotFound { .. })
    ));

    // Removing the main or sort-frequency dictionary clears the setting.
    store.remove(ids[2]).unwrap();
    store.remove(ids[1]).unwrap();
    let lookup = store.lookup_settings().unwrap();
    assert_eq!(lookup.main_dictionary, None);
    assert_eq!(lookup.sort_frequency_dictionary, None);
}

#[test]
fn metadata_queries_follow_indexeddb_order() {
    let dir = TempDir::new("store-metadata");
    let mut store = open(&dir);
    let zip = fixture::dictionary(
        &fixture::index("M"),
        &[
            (
                "term_meta_bank_1.json",
                r#"[["b","freq",1],["a","freq",2],["b","freq",3],["c","freq",4]]"#,
            ),
            (
                "tag_bank_1.json",
                r#"[["t","one",1,"first",0],["t","two",2,"second",0],["u","",0,"",0]]"#,
            ),
            (
                "kanji_bank_1.json",
                r#"[["字","ジ","あざ","",["character"],{"z":"1","10":"2","a":"3","2":"4"}]]"#,
            ),
            (
                "kanji_meta_bank_1.json",
                r#"[["字","freq",5],["字","freq","6"]]"#,
            ),
            (
                "term_bank_1.json",
                r#"[["x","","","",0,["same"],1,""],["y","","","",0,["same"],1,""],["z","","","",0,["other"],1,""]]"#,
            ),
        ],
    );
    import(&mut store, &dir, "m.zip", zip).unwrap();
    let id = store.lookup_dictionaries().unwrap()[0].id;

    let metas = store
        .find_term_meta(&["a".to_string(), "b".to_string()], &[id])
        .unwrap();
    let got: Vec<(usize, String, serde_json::Value)> = metas
        .into_iter()
        .map(|m| (m.index, m.term, m.data))
        .collect();
    assert_eq!(
        got,
        vec![
            (0, "a".to_string(), json!(2)),
            (1, "b".to_string(), json!(1)),
            (1, "b".to_string(), json!(3)),
        ]
    );
    assert!(
        store
            .find_term_meta(&["a".to_string()], &[id + 1])
            .unwrap()
            .is_empty()
    );

    // The first tag row with a name wins, as findTagMetaBulk.
    let tags = store
        .find_tags(&[
            (id, "t".to_string()),
            (id, "missing".to_string()),
            (id, "u".to_string()),
        ])
        .unwrap();
    assert_eq!(tags[0].as_ref().unwrap().category, "one");
    assert!(tags[1].is_none());
    assert_eq!(tags[2].as_ref().unwrap().category, "");

    // Kanji stats in Object.entries order: integer keys first.
    let kanji = store.find_kanji(&["字".to_string()], &[id]).unwrap();
    let names: Vec<&str> = kanji[0].stats.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["2", "10", "z", "a"]);
    assert_eq!(kanji[0].meanings, ["character"]);
    let metas = store.find_kanji_meta(&["字".to_string()], &[id]).unwrap();
    assert_eq!(metas.len(), 2);

    // Same content, same hash; sizes come from the record headers.
    let ids: Vec<i64> = store
        .find_terms(
            &["x".to_string(), "y".to_string(), "z".to_string()],
            None,
            10,
        )
        .unwrap()
        .iter()
        .map(|h| h.id)
        .collect();
    let sizes = store.glossary_sizes(&ids).unwrap();
    // Glossary and plain text JSON.
    assert_eq!(sizes[&ids[0]], 2 * r#"["same"]"#.len());
    let items: Vec<(i64, bool)> = ids.iter().map(|&id| (id, false)).collect();
    let hashes = store.glossary_content_hashes(&items).unwrap();
    assert_eq!(hashes[&ids[0]], hashes[&ids[1]]);
    assert_ne!(hashes[&ids[0]], hashes[&ids[2]]);
    assert_eq!(
        store.glossary_content(ids[1], false).unwrap(),
        r#"["same"]"#
    );

    let by_sequence = store.find_terms_by_sequence(&[(1, id)]).unwrap();
    assert_eq!(by_sequence.len(), 3);
    let exact = store
        .find_terms_exact(&[("y".to_string(), "y".to_string())], &[id])
        .unwrap();
    assert_eq!(exact.len(), 1);
    assert_eq!(exact[0].1.term, "y");
}

#[test]
fn cancelled_import_writes_nothing() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dir = TempDir::new("store-cancel");
    let mut store = open(&dir);
    let path = dir.write("a.zip", &fixture::terms_dictionary("A"));
    let cancel = AtomicBool::new(false);
    // Cancel after the first bank file.
    let err = store
        .import_cancellable(
            &path,
            &Limits::default(),
            &mut |_| cancel.store(true, Ordering::Relaxed),
            &cancel,
        )
        .unwrap_err();
    assert!(matches!(err, StoreError::Cancelled), "{err}");
    assert!(store.dictionaries().unwrap().is_empty());
    assert_eq!(store.check_integrity().unwrap(), Vec::<String>::new());
    // Not cancelled: imports normally.
    cancel.store(false, Ordering::Relaxed);
    store
        .import_cancellable(&path, &Limits::default(), &mut |_| {}, &cancel)
        .unwrap();
    assert_eq!(store.dictionaries().unwrap().len(), 1);
}

/// The glossary renderer and the lookup code recurse over structured
/// content; nesting is bounded because parsing at import refuses JSON
/// deeper than serde_json's limit (128).
#[test]
fn deeply_nested_glossary_is_refused_at_import() {
    let dir = TempDir::new("store-deep");
    let mut store = open(&dir);
    let depth = 200;
    let content = format!(
        "{}\"x\"{}",
        r#"{"tag":"span","content":"#.repeat(depth),
        "}".repeat(depth)
    );
    let bank = format!(
        r#"[["t","t","","",0,[{{"type":"structured-content","content":{content}}}],0,""]]"#
    );
    let zip = fixture::dictionary(&fixture::index("Deep"), &[("term_bank_1.json", &bank)]);
    let err = import(&mut store, &dir, "deep.zip", zip).unwrap_err();
    assert!(err.to_string().contains("recursion limit"), "{err}");
    assert!(store.dictionaries().unwrap().is_empty());
}
