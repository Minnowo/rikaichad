// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exports a store as the rows Yomitan's IndexedDB would hold, for
//! scripts/port/yomitan-oracle.mjs (docs/plan/milestone-3.md): JSON lines,
//! first {"settings", "dictionaries"}, then one ["table", row] per row, ids
//! kept. Ignored by default; run with
//!
//! JISHO_EXPORT_STORE=<store> JISHO_EXPORT_OUT=<rows.jsonl> \
//! cargo test --release -p jisho-store --test export_oracle -- --ignored

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use jisho_store::Store;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};

#[test]
#[ignore = "needs JISHO_EXPORT_STORE and JISHO_EXPORT_OUT"]
fn export_rows_for_oracle() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name}"));
    let path = PathBuf::from(var("JISHO_EXPORT_STORE"));
    let store = Store::open(&path).unwrap();
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut out = std::io::BufWriter::new(std::fs::File::create(var("JISHO_EXPORT_OUT")).unwrap());

    let meta: HashMap<String, String> = conn
        .prepare("SELECT key, value FROM meta")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut titles: HashMap<i64, String> = HashMap::new();
    let mut dictionaries = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT id, title, enabled, alias, allow_secondary_searches, parts_of_speech_filter,
                    use_deinflections, index_json FROM dictionaries ORDER BY priority, id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, bool>(5)?,
                r.get::<_, bool>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .unwrap();
    for row in rows {
        let (id, title, enabled, alias, secondary, pos, deinflections, index) = row.unwrap();
        let index: Value = serde_json::from_str(&index).unwrap();
        titles.insert(id, title.clone());
        dictionaries.push(json!({
            "title": title,
            "enabled": enabled,
            "alias": alias.filter(|a| !a.is_empty()).unwrap_or_else(|| title.clone()),
            "allowSecondarySearches": secondary,
            "partsOfSpeechFilter": pos,
            "useDeinflections": deinflections,
            "frequencyMode": index.get("frequencyMode").cloned().unwrap_or(Value::Null),
        }));
    }
    let settings = json!({
        "mainDictionary": meta.get("main_dictionary").cloned().unwrap_or_default(),
        "sortFrequencyDictionary": meta.get("sort_frequency_dictionary"),
        "sortFrequencyDictionaryOrder": meta.get("sort_frequency_order").map_or("descending", String::as_str),
    });
    writeln!(
        out,
        "{}",
        json!({"settings": settings, "dictionaries": dictionaries})
    )
    .unwrap();

    // Terms, with glossaries decompressed in batches.
    let mut stmt = conn
        .prepare(
            "SELECT id, dictionary, expression, reading, definition_tags, rules, score, sequence,
                    term_tags FROM terms ORDER BY id",
        )
        .unwrap();
    let mut rows = stmt.query([]).unwrap();
    let mut batch: Vec<Value> = Vec::new();
    let flush = |batch: &mut Vec<Value>, out: &mut dyn Write| {
        let ids: Vec<i64> = batch.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        let glossaries = store.load_glossaries(&ids).unwrap();
        for mut row in batch.drain(..) {
            let id = row["id"].as_i64().unwrap();
            row["glossary"] = json!(glossaries[&id]);
            writeln!(out, "{}", json!(["terms", row])).unwrap();
        }
    };
    while let Some(r) = rows.next().unwrap() {
        batch.push(json!({
            "id": r.get::<_, i64>(0).unwrap(),
            "dictionary": titles[&r.get::<_, i64>(1).unwrap()],
            "expression": r.get::<_, String>(2).unwrap(),
            "reading": r.get::<_, String>(3).unwrap(),
            "definitionTags": r.get::<_, String>(4).unwrap(),
            "rules": r.get::<_, String>(5).unwrap(),
            "score": r.get::<_, f64>(6).unwrap(),
            "sequence": r.get::<_, i64>(7).unwrap(),
            "termTags": r.get::<_, String>(8).unwrap(),
        }));
        if batch.len() == 2000 {
            flush(&mut batch, &mut out);
        }
    }
    flush(&mut batch, &mut out);

    let mut table = |name: &str, sql: &str, convert: &dyn Fn(&rusqlite::Row<'_>) -> Value| {
        let mut stmt = conn.prepare(sql).unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            writeln!(out, "{}", json!([name, convert(r)])).unwrap();
        }
    };
    let title = |r: &rusqlite::Row<'_>| titles[&r.get::<_, i64>(1).unwrap()].clone();
    let parse = |text: String| serde_json::from_str::<Value>(&text).unwrap();
    table(
        "termMeta",
        "SELECT id, dictionary, expression, mode, data FROM term_meta ORDER BY id",
        &|r| json!({"id": r.get::<_, i64>(0).unwrap(), "dictionary": title(r), "expression": r.get::<_, String>(2).unwrap(), "mode": r.get::<_, String>(3).unwrap(), "data": parse(r.get(4).unwrap())}),
    );
    table(
        "kanji",
        "SELECT id, dictionary, character, onyomi, kunyomi, tags, meanings, stats FROM kanji ORDER BY id",
        &|r| {
            // Stats as pairs, in order (an object keeps that order in JS
            // except integer-like keys, which come first either way).
            let pairs: Vec<(String, String)> =
                serde_json::from_str(&r.get::<_, String>(7).unwrap()).unwrap();
            let stats: serde_json::Map<String, Value> =
                pairs.into_iter().map(|(k, v)| (k, json!(v))).collect();
            json!({"id": r.get::<_, i64>(0).unwrap(), "dictionary": title(r), "character": r.get::<_, String>(2).unwrap(), "onyomi": r.get::<_, String>(3).unwrap(), "kunyomi": r.get::<_, String>(4).unwrap(), "tags": r.get::<_, String>(5).unwrap(), "meanings": parse(r.get(6).unwrap()), "stats": stats})
        },
    );
    table(
        "kanjiMeta",
        "SELECT id, dictionary, character, mode, data FROM kanji_meta ORDER BY id",
        &|r| json!({"id": r.get::<_, i64>(0).unwrap(), "dictionary": title(r), "character": r.get::<_, String>(2).unwrap(), "mode": r.get::<_, String>(3).unwrap(), "data": parse(r.get(4).unwrap())}),
    );
    table(
        "tagMeta",
        "SELECT id, dictionary, name, category, sort_order, notes, score FROM tags ORDER BY id",
        &|r| json!({"id": r.get::<_, i64>(0).unwrap(), "dictionary": title(r), "name": r.get::<_, String>(2).unwrap(), "category": r.get::<_, String>(3).unwrap(), "order": r.get::<_, f64>(4).unwrap(), "notes": r.get::<_, String>(5).unwrap(), "score": r.get::<_, f64>(6).unwrap()}),
    );
}
