// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parity with Yomitan's translator tests (tests/fixtures/yomitan,
//! copied from Yomitan's test/data): every Japanese findTerms case (all
//! result modes) and every findKanji case, comparing whole entries. Cases
//! using textReplacements or other languages are out of jisho's scope.

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
    DictionaryOptions, EnabledDictionaries, FindTermsOptions, KanjiEntry, PronunciationKind,
    ResultMode, Tag, TermEntry, find_kanji, find_terms, load_glossaries,
};
use jisho_store::Store;
use jisho_testkit::{Method, TempDir, ZipBuilder};
use serde_json::{Map, Value, json};

const DICTIONARY_NAME: &str = "Test Dictionary 2";
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/yomitan");
/// Yomitan rewrites image glossary items at import with the image file's
/// real size; jisho does not read media (text only, brief section 4 item
/// 11), so these keys of `type: "image"` items are not compared.
const IMAGE_SIZE_KEYS: [&str; 4] = ["width", "height", "preferredWidth", "preferredHeight"];

#[test]
fn results_match_yomitan() {
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

    let mut compared: HashMap<String, usize> = HashMap::new();
    let mut failures = Vec::new();
    for case in inputs["tests"].as_array().unwrap() {
        let func = case["func"].as_str().unwrap();
        let options = Value::Object(merged_options(&inputs["optionsPresets"], &case["options"]));
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let expected = expected_by_name[name];
        let dictionaries = enabled(&options, dictionary_id);
        let remove_non_japanese = options["removeNonJapaneseCharacters"]
            .as_bool()
            .unwrap_or(false);
        let (actual, expected, label) = match func {
            "findTerms" => {
                if options["language"] != "ja" || has_text_replacements(&options) {
                    continue;
                }
                let mode = case["mode"].as_str().unwrap();
                let find_options = FindTermsOptions {
                    mode: ResultMode::parse(mode).unwrap(),
                    remove_non_japanese_characters: remove_non_japanese,
                    deinflect: options["deinflect"].as_bool().unwrap_or(true),
                    primary_reading: options["primaryReading"].as_str().unwrap_or("").to_string(),
                    ..FindTermsOptions::default()
                };
                let mut result = find_terms(&store, text, &dictionaries, &find_options).unwrap();
                let filled = load_glossaries(&store, &mut result.entries, usize::MAX).unwrap();
                assert_eq!(filled, result.entries.len());
                let actual = json!({
                    "originalTextLength": result.original_text_length,
                    "dictionaryEntries": result.entries.iter().map(entry_json).collect::<Vec<_>>(),
                });
                let expected = json!({
                    "originalTextLength": expected["originalTextLength"],
                    "dictionaryEntries": expected["dictionaryEntries"],
                });
                (actual, expected, mode.to_string())
            }
            "findKanji" => {
                let entries = find_kanji(&store, text, &dictionaries, remove_non_japanese).unwrap();
                let actual = json!({
                    "dictionaryEntries": entries.iter().map(kanji_json).collect::<Vec<_>>(),
                });
                let expected = json!({"dictionaryEntries": expected["dictionaryEntries"]});
                (actual, expected, "kanji".to_string())
            }
            other => panic!("unknown func {other}"),
        };
        let actual = normalize(&strip_image_sizes(&actual));
        let expected = normalize(&strip_image_sizes(&expected));
        if actual != expected {
            failures.push(format!(
                "{name} ({text}):\n  expected {}\n  actual   {}",
                serde_json::to_string(&expected).unwrap(),
                serde_json::to_string(&actual).unwrap()
            ));
        }
        *compared.entry(label).or_default() += 1;
    }
    let mut counts: Vec<(String, usize)> = compared.into_iter().collect();
    counts.sort();
    assert_eq!(
        counts,
        vec![
            ("group".to_string(), 1),
            ("kanji".to_string(), 3),
            ("merge".to_string(), 2),
            ("simple".to_string(), 1),
            ("split".to_string(), 32),
            ("term".to_string(), 1),
        ],
        "number of compared cases changed"
    );
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
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
    let mut dictionaries = EnabledDictionaries::default();
    for pair in options["enabledDictionaryMap"].as_array().unwrap() {
        let settings = &pair[1];
        dictionaries.by_id.insert(
            dictionary_id,
            DictionaryOptions {
                index: settings["index"].as_u64().unwrap() as usize,
                alias: settings["alias"].as_str().unwrap().to_string(),
                title: DICTIONARY_NAME.to_string(),
                allow_secondary_searches: settings["allowSecondarySearches"]
                    .as_bool()
                    .unwrap_or(false),
                parts_of_speech_filter: settings["partsOfSpeechFilter"].as_bool().unwrap_or(true),
                use_deinflections: settings["useDeinflections"].as_bool().unwrap_or(true),
            },
        );
    }
    if options["mainDictionary"] == "${title}" {
        dictionaries.main_dictionary = Some((dictionary_id, DICTIONARY_NAME.to_string()));
    }
    assert!(options["sortFrequencyDictionary"].is_null());
    dictionaries
}

/// Yomitan's getCompositePreset: a preset by name, or a list of preset
/// names and override objects applied in order.
fn merged_options(presets: &Value, options: &Value) -> Map<String, Value> {
    let mut merged = Map::new();
    let items = match options {
        Value::Array(list) => list.clone(),
        other => vec![other.clone()],
    };
    for item in items {
        let overrides = match &item {
            Value::String(preset) => presets[preset.as_str()].as_object().unwrap().clone(),
            Value::Object(map) => map.clone(),
            other => panic!("bad options item {other}"),
        };
        for (key, value) in overrides {
            merged.insert(key, value);
        }
    }
    merged
}

fn has_text_replacements(options: &Value) -> bool {
    options["textReplacements"]
        .as_array()
        .is_some_and(|list| list.iter().any(|v| !v.is_null()))
}

fn tags_json(tags: &[Tag]) -> Value {
    json!(
        tags.iter()
            .map(|t| json!({
                "name": t.name,
                "category": t.category,
                "order": t.order,
                "score": t.score,
                "content": t.content,
                "dictionaries": t.dictionaries,
                "redundant": t.redundant,
            }))
            .collect::<Vec<_>>()
    )
}

fn kanji_json(entry: &KanjiEntry) -> Value {
    let stats: Map<String, Value> = entry
        .stats
        .iter()
        .map(|(category, stats)| {
            (
                category.clone(),
                json!(
                    stats
                        .iter()
                        .map(|s| json!({
                            "name": s.name,
                            "category": s.category,
                            "content": s.content,
                            "order": s.order,
                            "score": s.score,
                            "dictionary": s.dictionary,
                            "value": s.value,
                        }))
                        .collect::<Vec<_>>()
                ),
            )
        })
        .collect();
    json!({
        "type": "kanji",
        "character": entry.character,
        "dictionary": entry.dictionary,
        "dictionaryIndex": entry.dictionary_index,
        "dictionaryAlias": entry.dictionary_alias,
        "onyomi": entry.onyomi,
        "kunyomi": entry.kunyomi,
        "tags": tags_json(&entry.tags),
        "stats": stats,
        "definitions": entry.definitions,
        "frequencies": entry.frequencies.iter().map(|f| json!({
            "index": f.index,
            "dictionary": f.dictionary,
            "dictionaryIndex": f.dictionary_index,
            "dictionaryAlias": f.dictionary_alias,
            "character": f.character,
            "frequency": f.frequency,
            "displayValue": f.display_value,
            "displayValueParsed": f.display_value_parsed,
            "frequencyMode": f.frequency_mode,
        })).collect::<Vec<_>>(),
    })
}

/// A TermEntry in Yomitan's JSON shape.
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
            "tags": tags_json(&h.tags),
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
            "tags": tags_json(&d.tags),
            "entries": d.entries,
        })).collect::<Vec<_>>(),
        "pronunciations": entry.pronunciations.iter().map(|p| json!({
            "index": p.index,
            "headwordIndex": p.headword_index,
            "dictionary": p.dictionary,
            "dictionaryIndex": p.dictionary_index,
            "dictionaryAlias": p.dictionary_alias,
            "pronunciations": p.pronunciations.iter().map(|item| match &item.kind {
                PronunciationKind::PitchAccent { positions, nasal_positions, devoice_positions } => json!({
                    "type": "pitch-accent",
                    "positions": positions,
                    "nasalPositions": nasal_positions,
                    "devoicePositions": devoice_positions,
                    "tags": tags_json(&item.tags),
                }),
                PronunciationKind::PhoneticTranscription { ipa } => json!({
                    "type": "phonetic-transcription",
                    "ipa": ipa,
                    "tags": tags_json(&item.tags),
                }),
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "frequencies": entry.frequencies.iter().map(|f| json!({
            "index": f.index,
            "headwordIndex": f.headword_index,
            "dictionary": f.dictionary,
            "frequencyMode": f.frequency_mode,
            "dictionaryIndex": f.dictionary_index,
            "dictionaryAlias": f.dictionary_alias,
            "hasReading": f.has_reading,
            "frequency": f.frequency,
            "displayValue": f.display_value,
            "displayValueParsed": f.display_value_parsed,
        })).collect::<Vec<_>>(),
    })
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
