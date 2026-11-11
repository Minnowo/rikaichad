// SPDX-License-Identifier: AGPL-3.0-or-later
//! Furigana distribution against Yomitan's own functions on generated
//! inputs (tests/fixtures/yomitan/furigana-golden.json, made by
//! scripts/port/dump-furigana-golden.mjs).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use rikaichad_ja::{FuriganaSegment, distribute_furigana, distribute_furigana_inflected};
use serde_json::Value;

#[test]
fn furigana_matches_yomitan() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/yomitan/furigana-golden.json"
    ))
    .unwrap();
    let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
    assert_eq!(cases.len(), 3000);
    let pairs = |segments: Vec<FuriganaSegment>| -> Value {
        segments
            .into_iter()
            .map(|s| serde_json::json!([s.text, s.reading]))
            .collect()
    };
    let mut failures = Vec::new();
    for case in &cases {
        let term = case["term"].as_str().unwrap();
        let reading = case["reading"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let plain = pairs(distribute_furigana(term, reading));
        if plain != case["furigana"] {
            failures.push(format!(
                "distribute({term}, {reading}): {plain} != {}",
                case["furigana"]
            ));
        }
        let inflected = pairs(distribute_furigana_inflected(term, reading, source));
        if inflected != case["inflected"] {
            failures.push(format!(
                "inflected({term}, {reading}, {source}): {inflected} != {}",
                case["inflected"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}
