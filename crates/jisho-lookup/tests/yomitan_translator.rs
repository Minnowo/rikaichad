// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parity with Yomitan's translator tests (tests/fixtures/yomitan,
//! copied from Yomitan's test/data): the Japanese "split"-mode cases,
//! comparing everything milestone 2 produces. Tags, frequencies and
//! pronunciations are milestone 3 and are left out of the comparison.
//! Cases using textReplacements or other languages are out of jisho's scope.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashMap;
use std::path::Path;

use jisho_format::Limits;
use jisho_lookup::{
    DictionaryOptions, EnabledDictionaries, FindTermsOptions, TermEntry, find_terms,
    load_glossaries,
};
use jisho_store::Store;
use jisho_testkit::{Method, TempDir, ZipBuilder};
use serde_json::{Map, Value, json};

const DICTIONARY_NAME: &str = "Test Dictionary 2";
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/yomitan");
/// Keys compared in milestone 3, not now.
const LATER_KEYS: [&str; 3] = ["tags", "frequencies", "pronunciations"];
/// Yomitan rewrites image glossary items at import with the image file's
/// real size; jisho does not read media (text only, brief section 4 item
/// 11), so these keys of `type: "image"` items are not compared.
const IMAGE_SIZE_KEYS: [&str; 4] = ["width", "height", "preferredWidth", "preferredHeight"];

#[test]
fn split_mode_results_match_yomitan() {
    let dir = TempDir::new("yomitan-translator");
    let mut store = Store::open(&dir.path().join("store.sqlite")).unwrap();
    let zip = dir.write(
        "dict.zip",
        &dictionary_zip(&Path::new(FIXTURES).join("valid-dictionary1")),
    );
    store.import(&zip, &Limits::default(), &mut |_| {}).unwrap();
    let dictionary_id = store.dictionaries().unwrap()[0].id;

    let inputs = read_json("translator-test-inputs.json");
    let results = read_json("translator-test-results.json");
    let expected_by_name: HashMap<&str, &Value> = results
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["name"].as_str().unwrap(), r))
        .collect();
    let preset = &inputs["optionsPresets"]["default"];

    let mut compared = 0;
    let mut failures = Vec::new();
    for case in inputs["tests"].as_array().unwrap() {
        if case["func"] != "findTerms" || case["mode"] != "split" {
            continue;
        }
        let options = Value::Object(merged_options(preset, &case["options"]));
        if options["language"] != "ja" || has_text_replacements(&options) {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let dictionaries = enabled(&options, dictionary_id);
        let find_options = FindTermsOptions {
            remove_non_japanese_characters: options["removeNonJapaneseCharacters"]
                .as_bool()
                .unwrap_or(false),
            deinflect: options["deinflect"].as_bool().unwrap_or(true),
            primary_reading: options["primaryReading"].as_str().unwrap_or("").to_string(),
            ..FindTermsOptions::default()
        };
        let mut result = find_terms(&store, text, &dictionaries, &find_options).unwrap();
        let filled = load_glossaries(&store, &mut result.entries, usize::MAX).unwrap();
        assert_eq!(filled, result.entries.len());

        let expected = expected_by_name[name];
        let actual = json!({
            "originalTextLength": result.original_text_length,
            "dictionaryEntries": result.entries.iter().map(entry_json).collect::<Vec<_>>(),
        });
        let expected = json!({
            "originalTextLength": expected["originalTextLength"],
            "dictionaryEntries": expected["dictionaryEntries"],
        });
        let actual = normalize(&strip_image_sizes(&actual));
        let expected = normalize(&strip_image_sizes(&strip_later(&expected)));
        if actual != expected {
            failures.push(format!(
                "{name} ({text}):\n  expected {}\n  actual   {}",
                serde_json::to_string(&expected).unwrap(),
                serde_json::to_string(&actual).unwrap()
            ));
        }
        compared += 1;
    }
    assert_eq!(compared, 32, "number of compared cases changed");
    assert!(
        failures.is_empty(),
        "{} of {compared} differ:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// The dictionary as Yomitan's createDictionaryArchiveData builds it: every
/// file of the directory in name order, JSON re-serialized compactly, the
/// index title replaced.
fn dictionary_zip(directory: &Path) -> Vec<u8> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    let mut zip = ZipBuilder::new();
    for name in names {
        let bytes = std::fs::read(directory.join(&name)).unwrap();
        let content = if name.ends_with(".json") {
            let mut value: Value = serde_json::from_slice(&bytes).unwrap();
            if name == "index.json" {
                value["title"] = json!(DICTIONARY_NAME);
            }
            serde_json::to_vec(&value).unwrap()
        } else {
            bytes
        };
        zip.add(&name, &content).method = Method::Stored;
    }
    zip.finish()
}

fn enabled(options: &Value, dictionary_id: i64) -> EnabledDictionaries {
    let mut by_id = HashMap::new();
    for pair in options["enabledDictionaryMap"].as_array().unwrap() {
        let settings = &pair[1];
        by_id.insert(
            dictionary_id,
            DictionaryOptions {
                index: settings["index"].as_u64().unwrap() as usize,
                alias: settings["alias"].as_str().unwrap().to_string(),
                title: DICTIONARY_NAME.to_string(),
                parts_of_speech_filter: settings["partsOfSpeechFilter"].as_bool().unwrap(),
                use_deinflections: settings["useDeinflections"].as_bool().unwrap(),
            },
        );
    }
    EnabledDictionaries { by_id }
}

/// Yomitan's getCompositePreset: the preset, then each override object.
fn merged_options(preset: &Value, options: &Value) -> Map<String, Value> {
    let mut merged = preset.as_object().unwrap().clone();
    if let Value::Array(list) = options {
        for item in list {
            if let Value::Object(overrides) = item {
                for (key, value) in overrides {
                    merged.insert(key.clone(), value.clone());
                }
            }
        }
    }
    merged
}

fn has_text_replacements(options: &Value) -> bool {
    options["textReplacements"]
        .as_array()
        .is_some_and(|list| list.iter().any(|v| !v.is_null()))
}

/// A TermEntry in Yomitan's JSON shape (fields compared in milestone 2).
fn entry_json(entry: &TermEntry) -> Value {
    json!({
        "type": "term",
        "isPrimary": entry.is_primary,
        "textProcessorRuleChainCandidates": entry.text_processor_rule_chain_candidates,
        "inflectionRuleChainCandidates": entry.inflection_rule_chain_candidates.iter().map(|chain| json!({
            "source": chain.source.as_str(),
            "inflectionRules": chain.inflection_rules.iter().map(|rule| {
                let mut value = json!({"name": rule.name});
                if let Some(description) = &rule.description {
                    value["description"] = json!(description);
                }
                value
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "score": entry.score,
        "frequencyOrder": entry.frequency_order,
        "dictionaryIndex": entry.dictionary_index,
        "dictionaryAlias": entry.dictionary_alias,
        "sourceTermExactMatchCount": entry.source_term_exact_match_count,
        "matchPrimaryReading": entry.match_primary_reading,
        "maxOriginalTextLength": entry.max_original_text_length,
        "headwords": entry.headwords.iter().map(|h| json!({
            "index": h.index,
            "headwordIndex": h.headword_index,
            "term": h.term,
            "reading": h.reading,
            "sources": h.sources.iter().map(|s| json!({
                "originalText": s.original_text,
                "transformedText": s.transformed_text,
                "deinflectedText": s.deinflected_text,
                "matchType": s.match_type,
                "matchSource": s.match_source,
                "isPrimary": s.is_primary,
            })).collect::<Vec<_>>(),
            "wordClasses": h.word_classes,
        })).collect::<Vec<_>>(),
        "definitions": entry.definitions.iter().map(|d| json!({
            "index": d.index,
            "headwordIndices": d.headword_indices,
            "dictionary": d.dictionary,
            "dictionaryIndex": d.dictionary_index,
            "dictionaryAlias": d.dictionary_alias,
            "id": d.id,
            "score": d.score,
            "frequencyOrder": d.frequency_order,
            "sequences": d.sequences,
            "isPrimary": d.is_primary,
            "entries": d.entries,
        })).collect::<Vec<_>>(),
    })
}

/// Removes keys compared only from milestone 3 on, at any depth.
fn strip_later(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !LATER_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), strip_later(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_later).collect()),
        other => other.clone(),
    }
}

/// Removes the media-derived size keys of image glossary items.
fn strip_image_sizes(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let is_image = map.get("type").and_then(Value::as_str) == Some("image");
            Value::Object(
                map.iter()
                    .filter(|(k, _)| !(is_image && IMAGE_SIZE_KEYS.contains(&k.as_str())))
                    .map(|(k, v)| (k.clone(), strip_image_sizes(v)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(strip_image_sizes).collect()),
        other => other.clone(),
    }
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

fn read_json(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(Path::new(FIXTURES).join(name)).unwrap()).unwrap()
}
