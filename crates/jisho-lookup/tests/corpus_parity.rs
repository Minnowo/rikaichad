// SPDX-License-Identifier: AGPL-3.0-or-later
//! Differential parity with Yomitan's own translator
//! (docs/plan/milestone-3.md step 6). Results are compared in a compact
//! form that scripts/port/yomitan-oracle.mjs also prints for Yomitan.
//!
//! `crafted_dictionary_matches_yomitan` compares with Yomitan's committed
//! output for a small dictionary (tests/fixtures/parity). The ignored
//! `print_compact_results` prints jisho's results for any store, to compare
//! with the oracle's using scripts/port/compare-parity.py:
//!
//! JISHO_PARITY_STORE=<store> JISHO_PARITY_TEXTS=<texts> \
//! JISHO_PARITY_MODES=split,group,merge,kanji JISHO_PARITY_OUT=<jisho.jsonl> \
//! cargo test --release -p jisho-lookup --test corpus_parity -- --ignored

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::{Path, PathBuf};

use jisho_format::Limits;
use jisho_lookup::{
    EnabledDictionaries, FindTermsOptions, KanjiEntry, PronunciationKind, ResultMode, Tag,
    TermEntry, find_kanji, find_terms, load_glossaries, parse_text,
};
use jisho_store::Store;
use jisho_testkit::{Method, TempDir, ZipBuilder};
use serde_json::{Value, json};

const PARITY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/parity");

#[test]
fn crafted_dictionary_matches_yomitan() {
    let dir = TempDir::new("parity-crafted");
    let source = Path::new(PARITY).join("merge-deinflection");
    let mut names: Vec<String> = std::fs::read_dir(&source)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    let mut zip = ZipBuilder::new();
    for name in &names {
        zip.add(name, &std::fs::read(source.join(name)).unwrap())
            .method = Method::Stored;
    }
    let path = dir.write("dict.zip", &zip.finish());
    let mut store = Store::open(&dir.path().join("store.sqlite")).unwrap();
    store
        .import(&path, &Limits::default(), &mut |_| {})
        .unwrap();
    let dictionaries = EnabledDictionaries::from_store(&store).unwrap();

    let expected =
        std::fs::read_to_string(Path::new(PARITY).join("merge-deinflection-yomitan.jsonl"))
            .unwrap();
    let mut compared = 0;
    for line in expected.lines() {
        let expected: Value = serde_json::from_str(line).unwrap();
        let mode = expected["mode"].as_str().unwrap();
        let text = expected["text"].as_str().unwrap();
        // Unlimited, and cut like Yomitan's maxResults (jisho then adds
        // tags and metadata only to the kept entries).
        for limit in [None, Some(1), Some(2)] {
            let actual = compact_lookup(&store, &dictionaries, mode, text, limit);
            let mut expected = expected.clone();
            if let (Some(limit), Some(entries)) = (
                limit,
                expected.get_mut("entries").and_then(Value::as_array_mut),
            ) {
                entries.truncate(limit);
            }
            assert_eq!(
                normalize(&actual),
                normalize(&expected),
                "{mode} {text} limit {limit:?}:\n  jisho   {actual}\n  yomitan {expected}"
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 54);
}

/// The public corpus (scripts/fetch-corpus.sh) against Yomitan's results
/// in tests/fixtures/parity/public-corpus-golden.json: the first 32
/// entries of each query in group and merge mode, by headwords and
/// inflection chains, in order.
#[test]
#[ignore = "needs the public corpus: scripts/fetch-corpus.sh (JISHO_CORPUS_DIR, default corpus/)"]
fn public_corpus_matches_golden() {
    let corpus = std::env::var_os("JISHO_CORPUS_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
        PathBuf::from,
    );
    let golden: Value = serde_json::from_str(
        &std::fs::read_to_string(Path::new(PARITY).join("public-corpus-golden.json")).unwrap(),
    )
    .unwrap();
    let dir = TempDir::new("parity-corpus");
    let mut store = Store::open(&dir.path().join("store.sqlite")).unwrap();
    for name in ["JMdict_english.zip", "KANJIDIC_english.zip"] {
        let path = corpus.join(name);
        let sha = jisho_store::sha256_hex(&mut std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            json!(sha),
            golden["corpus"][name],
            "{name} is not the pinned file"
        );
        store
            .import(&path, &Limits::default(), &mut |_| {})
            .unwrap();
    }
    let dictionaries = EnabledDictionaries::from_store(&store).unwrap();
    let max_results = golden["max_results"].as_u64().unwrap() as usize;
    let texts = golden["texts"].as_array().unwrap();
    let mut failures = Vec::new();
    for mode in ["group", "merge"] {
        let expected = golden["results"][mode].as_array().unwrap();
        for (text, expected) in texts.iter().zip(expected) {
            let text = text.as_str().unwrap();
            let options = FindTermsOptions {
                mode: ResultMode::parse(mode).unwrap(),
                limit: Some(max_results),
                ..FindTermsOptions::default()
            };
            let result = find_terms(&store, text, &dictionaries, &options).unwrap();
            let actual: Vec<Value> = result
                .entries
                .iter()
                .map(|e| {
                    let mut item: Vec<Value> = e
                        .headwords
                        .iter()
                        .map(|h| json!([h.term, h.reading]))
                        .collect();
                    item.push(json!(
                        e.inflection_rule_chain_candidates
                            .iter()
                            .map(|c| json!([
                                c.source.as_str(),
                                c.inflection_rules
                                    .iter()
                                    .map(|r| r.name.clone())
                                    .collect::<Vec<_>>(),
                            ]))
                            .collect::<Vec<_>>()
                    ));
                    json!(item)
                })
                .collect();
            if json!(actual) != *expected {
                failures.push(format!("{mode} {text}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ: {failures:?}",
        failures.len()
    );
}

/// Makes 1 and 1.0 compare equal.
fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), normalize(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        other => other.clone(),
    }
}

/// One line of the compact form: a lookup of `text` in `mode` (a result
/// mode, or "kanji").
fn compact_lookup(
    store: &Store,
    dictionaries: &EnabledDictionaries,
    mode: &str,
    text: &str,
    limit: Option<usize>,
) -> Value {
    if mode == "parse" {
        let tokens = parse_text(store, text, dictionaries, 16).unwrap();
        let tokens: Vec<Value> = tokens
            .iter()
            .map(|t| {
                json!([
                    t.segments
                        .iter()
                        .map(|s| json!([s.text, s.reading]))
                        .collect::<Vec<_>>(),
                    t.headwords
                        .iter()
                        .map(|list| list
                            .iter()
                            .map(|h| json!([h.term, h.reading]))
                            .collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                ])
            })
            .collect();
        return json!({"mode": mode, "text": text, "tokens": tokens});
    }
    if mode == "kanji" {
        let entries = find_kanji(store, text, dictionaries, false).unwrap();
        return json!({"mode": mode, "text": text, "kanji": entries.iter().map(compact_kanji).collect::<Vec<_>>()});
    }
    let options = FindTermsOptions {
        mode: ResultMode::parse(mode).expect("mode"),
        limit,
        ..FindTermsOptions::default()
    };
    let mut result = find_terms(store, text, dictionaries, &options).unwrap();
    load_glossaries(store, &mut result.entries, usize::MAX).unwrap();
    json!({
        "mode": mode,
        "text": text,
        "len": result.original_text_length,
        "entries": result.entries.iter().map(compact_entry).collect::<Vec<_>>(),
    })
}

#[test]
#[ignore = "needs JISHO_PARITY_* (a store and texts); see the module docs"]
fn print_compact_results() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name}"));
    let store = Store::open(&PathBuf::from(var("JISHO_PARITY_STORE"))).unwrap();
    let texts = std::fs::read_to_string(var("JISHO_PARITY_TEXTS")).unwrap();
    let modes = var("JISHO_PARITY_MODES");
    let mut out = std::io::BufWriter::new(std::fs::File::create(var("JISHO_PARITY_OUT")).unwrap());
    let dictionaries = EnabledDictionaries::from_store(&store).unwrap();
    for mode in modes.split(',') {
        for text in texts.lines().filter(|l| !l.is_empty()) {
            writeln!(
                out,
                "{}",
                compact_lookup(&store, &dictionaries, mode, text, None)
            )
            .unwrap();
        }
    }
}

fn tag(t: &Tag) -> Value {
    json!([
        t.name,
        t.category,
        t.order,
        t.score,
        t.content,
        t.dictionaries,
        t.redundant
    ])
}

fn tags(list: &[Tag]) -> Vec<Value> {
    list.iter().map(tag).collect()
}

fn compact_entry(e: &TermEntry) -> Value {
    json!({
        "h": e.headwords.iter().map(|h| json!([
            h.term, h.reading,
            h.sources.iter().map(|s| json!([s.original_text, s.transformed_text, s.deinflected_text, s.match_source, s.is_primary])).collect::<Vec<_>>(),
            tags(&h.tags), h.word_classes,
        ])).collect::<Vec<_>>(),
        "d": e.definitions.iter().map(|d| json!([
            d.dictionary, d.id, d.sequences, d.is_primary, d.headword_indices, tags(&d.tags),
            d.entries.len(), d.entries.iter().filter(|x| x.is_array()).count(),
            d.score, d.dictionary_index, d.dictionary_alias, d.frequency_order,
        ])).collect::<Vec<_>>(),
        "i": e.inflection_rule_chain_candidates.iter().map(|c| json!([
            c.source.as_str(), c.inflection_rules.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
        ])).collect::<Vec<_>>(),
        "tp": e.text_processor_rule_chain_candidates,
        "s": e.score, "di": e.dictionary_index, "da": e.dictionary_alias, "fo": e.frequency_order,
        "pr": e.is_primary, "ex": e.source_term_exact_match_count, "mx": e.max_original_text_length,
        "f": e.frequencies.iter().map(|f| json!([
            f.index, f.headword_index, f.dictionary, f.dictionary_index, f.has_reading, f.frequency,
            f.display_value, f.display_value_parsed, f.frequency_mode,
        ])).collect::<Vec<_>>(),
        "p": e.pronunciations.iter().map(|p| json!([
            p.index, p.headword_index, p.dictionary, p.dictionary_index,
            p.pronunciations.iter().map(|x| match &x.kind {
                PronunciationKind::PitchAccent { positions, nasal_positions, devoice_positions } =>
                    json!(["pitch", positions, nasal_positions, devoice_positions, tags(&x.tags)]),
                PronunciationKind::PhoneticTranscription { ipa } => json!(["ipa", ipa, tags(&x.tags)]),
            }).collect::<Vec<_>>(),
        ])).collect::<Vec<_>>(),
    })
}

fn compact_kanji(e: &KanjiEntry) -> Value {
    json!([
        e.character,
        e.dictionary,
        e.dictionary_index,
        e.dictionary_alias,
        e.onyomi,
        e.kunyomi,
        tags(&e.tags),
        e.stats
            .iter()
            .map(|(category, list)| json!([
                category,
                list.iter()
                    .map(|s| json!([s.name, s.category, s.content, s.order, s.score, s.value]))
                    .collect::<Vec<_>>(),
            ]))
            .collect::<Vec<_>>(),
        e.definitions,
        e.frequencies
            .iter()
            .map(|f| json!([
                f.index,
                f.dictionary,
                f.dictionary_index,
                f.frequency,
                f.display_value,
                f.display_value_parsed,
                f.frequency_mode,
            ]))
            .collect::<Vec<_>>(),
    ])
}
