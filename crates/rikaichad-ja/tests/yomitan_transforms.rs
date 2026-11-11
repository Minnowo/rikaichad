// Ported from Yomitan test/fixtures/language-transformer-test.js and
// test/language/japanese-transforms.test.js at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2024-2026  Yomitan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Yomitan's Japanese deinflection tests against the ported transformer.
//! The cases are in tests/fixtures/yomitan/japanese-transforms-tests.json;
//! the matcher is a port of hasTermReasons.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rikaichad_ja::{condition_flags_from_condition_type, conditions_match, transform};
use serde_json::Value;

#[test]
fn yomitan_japanese_transform_tests_pass() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/yomitan/japanese-transforms-tests.json"
    );
    let data: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    let mut checked = 0;
    for category in data.as_array().unwrap() {
        let name = category["category"].as_str().unwrap();
        let valid = category["valid"].as_bool().unwrap();
        for case in category["tests"].as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let term = case["term"].as_str().unwrap();
            let rule = case["rule"].as_str();
            let reasons: Option<Vec<&str>> = case["reasons"]
                .as_array()
                .map(|r| r.iter().map(|x| x.as_str().unwrap()).collect());
            checked += 1;
            if has_term_reasons(source, term, rule, reasons.as_deref()) != valid {
                failures.push(format!(
                    "[{name}] {source} -> {term} rule={rule:?} reasons={reasons:?} expected {valid}"
                ));
            }
        }
    }
    assert_eq!(checked, 1446, "test data changed");
    assert!(
        failures.is_empty(),
        "{} of {checked} failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// hasTermReasons: some candidate equals `term`, its conditions match
/// `rule` (if given) and its trace equals `reasons` (if given).
fn has_term_reasons(
    source: &str,
    term: &str,
    rule: Option<&str>,
    reasons: Option<&[&str]>,
) -> bool {
    for candidate in transform(source) {
        if candidate.text != term {
            continue;
        }
        if let Some(rule) = rule {
            let expected = condition_flags_from_condition_type(rule);
            if !conditions_match(candidate.conditions, expected) {
                continue;
            }
        }
        if let Some(reasons) = reasons {
            let trace: Vec<&str> = candidate.trace.iter().map(|f| f.transform).collect();
            if trace != reasons {
                continue;
            }
        }
        return true;
    }
    false
}
