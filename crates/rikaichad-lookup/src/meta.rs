// Ported from Yomitan ext/js/language/translator.js (_addTermMeta,
// _getFrequencyInfo, _convertStringToNumber, _toNumberArray,
// _updateSortFrequencies, _createTermFrequency, _createTermPronunciation)
// at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Frequencies and pronunciations of headwords, and the frequency order
//! used for sorting.

use std::collections::HashMap;

use rikaichad_store::{FrequencyOrder, Store, StoreError};
use serde_json::Value;

use crate::entry::{Pronunciation, PronunciationKind, TermEntry, TermFrequency, TermPronunciation};
use crate::options::EnabledDictionaries;
use crate::tags::TagRegistry;

/// The readings of one term, each with its (entry, headword) targets.
type ReadingTargets = Vec<(String, Vec<(usize, usize)>)>;

/// The order in which Yomitan's _addTermMeta meets headword terms, and
/// each term's readings, walking all entries before sorting. Metadata is
/// numbered (`index`) in that order, so rikaichad, which adds metadata only to
/// the entries it keeps, records the order before cutting.
#[derive(Debug, Default)]
pub(crate) struct HeadwordOrder {
    terms: HashMap<String, usize>,
    readings: HashMap<(String, String), usize>,
}

impl HeadwordOrder {
    pub fn of(entries: &[TermEntry]) -> HeadwordOrder {
        let mut order = HeadwordOrder::default();
        for headword in entries.iter().flat_map(|e| &e.headwords) {
            let next = order.terms.len();
            order.terms.entry(headword.term.clone()).or_insert(next);
            let next = order.readings.len();
            order
                .readings
                .entry((headword.term.clone(), headword.reading.clone()))
                .or_insert(next);
        }
        order
    }
}

/// Which metadata [`add_term_meta`] adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetaKinds {
    /// Frequencies, pitch accents and transcriptions.
    All,
    /// Frequencies only (enough to compute sort orders).
    FrequenciesOnly,
}

/// _addTermMeta: adds the frequencies and pronunciations that the
/// dictionaries `dictionary_ids` have for the entries' headwords.
pub(crate) fn add_term_meta(
    store: &Store,
    entries: &mut [TermEntry],
    dictionaries: &EnabledDictionaries,
    dictionary_ids: &[i64],
    kinds: MetaKinds,
    order: Option<&HeadwordOrder>,
    registry: &mut TagRegistry,
) -> Result<(), StoreError> {
    // term -> reading -> targets (entry, headword index), in first-seen order.
    let mut terms: Vec<String> = Vec::new();
    let mut readings: Vec<ReadingTargets> = Vec::new();
    let mut term_positions: HashMap<String, usize> = HashMap::new();
    for (entry_index, entry) in entries.iter().enumerate() {
        for (headword_index, headword) in entry.headwords.iter().enumerate() {
            let position = *term_positions
                .entry(headword.term.clone())
                .or_insert_with(|| {
                    terms.push(headword.term.clone());
                    readings.push(Vec::new());
                    terms.len() - 1
                });
            let Some(reading_map) = readings.get_mut(position) else {
                continue;
            };
            let target = (entry_index, headword_index);
            match reading_map.iter_mut().find(|(r, _)| *r == headword.reading) {
                Some((_, targets)) => targets.push(target),
                None => reading_map.push((headword.reading.clone(), vec![target])),
            }
        }
    }

    if let Some(order) = order {
        // Terms and readings in the order of all entries before sorting.
        let rank = |term: &String| order.terms.get(term).copied().unwrap_or(usize::MAX);
        let mut paired: Vec<(String, ReadingTargets)> = terms.into_iter().zip(readings).collect();
        paired.sort_by_key(|(term, _)| rank(term));
        for (term, reading_map) in &mut paired {
            reading_map.sort_by_key(|(reading, _)| {
                order
                    .readings
                    .get(&(term.clone(), reading.clone()))
                    .copied()
                    .unwrap_or(usize::MAX)
            });
        }
        (terms, readings) = paired.into_iter().unzip();
    }
    let metas = store.find_term_meta(&terms, dictionary_ids)?;
    for meta in metas {
        let dictionary_index = dictionaries.index_of(meta.dictionary_id);
        let title = dictionaries.title_of(meta.dictionary_id);
        let dictionary_alias = dictionaries.alias_of(meta.dictionary_id, &title);
        let Some(reading_map) = readings.get(meta.index) else {
            continue;
        };
        match meta.mode.as_str() {
            "freq" => {
                let reading_of_data = match &meta.data {
                    Value::Object(map) => map.get("reading").and_then(Value::as_str),
                    _ => None,
                };
                let frequency = match (&meta.data, reading_of_data) {
                    (Value::Object(map), Some(_)) => map.get("frequency").unwrap_or(&Value::Null),
                    (data, _) => data,
                };
                let info = frequency_info(frequency);
                for (reading, targets) in reading_map {
                    if reading_of_data.is_some_and(|r| r != reading) {
                        continue;
                    }
                    for &(entry_index, headword_index) in targets {
                        let Some(entry) = entries.get_mut(entry_index) else {
                            continue;
                        };
                        let index = entry.frequencies.len();
                        entry.frequencies.push(TermFrequency {
                            index,
                            headword_index,
                            dictionary: title.clone(),
                            dictionary_id: meta.dictionary_id,
                            dictionary_index,
                            dictionary_alias: dictionary_alias.clone(),
                            has_reading: reading_of_data.is_some(),
                            frequency: info.frequency,
                            display_value: info.display_value.clone(),
                            display_value_parsed: info.display_value_parsed,
                            frequency_mode: dictionaries
                                .frequency_modes
                                .get(&meta.dictionary_id)
                                .cloned(),
                        });
                    }
                }
            }
            "pitch" | "ipa" if kinds == MetaKinds::All => {
                let data_reading = meta.data.get("reading").and_then(Value::as_str);
                for (reading, targets) in reading_map {
                    if data_reading != Some(reading.as_str()) {
                        continue;
                    }
                    // Built once per reading, as Yomitan shares one list
                    // between the targets; tag order only depends on the
                    // registry, so building per reading is the same.
                    let items = pronunciations(
                        &meta.data,
                        &meta.mode,
                        meta.dictionary_id,
                        &title,
                        registry,
                    );
                    for &(entry_index, headword_index) in targets {
                        let Some(entry) = entries.get_mut(entry_index) else {
                            continue;
                        };
                        let index = entry.pronunciations.len();
                        entry.pronunciations.push(TermPronunciation {
                            index,
                            headword_index,
                            dictionary: title.clone(),
                            dictionary_id: meta.dictionary_id,
                            dictionary_index,
                            dictionary_alias: dictionary_alias.clone(),
                            pronunciations: items.clone(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The pitch accents or transcriptions of one pitch/ipa meta row.
fn pronunciations(
    data: &Value,
    mode: &str,
    dictionary_id: i64,
    dictionary: &str,
    registry: &mut TagRegistry,
) -> Vec<Pronunciation> {
    let list_key = if mode == "pitch" {
        "pitches"
    } else {
        "transcriptions"
    };
    let empty = Vec::new();
    let items = data
        .get(list_key)
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let mut tag_groups = Vec::new();
        if let Some(Value::Array(tags)) = item.get("tags") {
            let names: Vec<String> = tags
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            registry.add(&mut tag_groups, dictionary_id, dictionary, &names);
        }
        let kind = if mode == "pitch" {
            PronunciationKind::PitchAccent {
                positions: item.get("position").cloned().unwrap_or(Value::Null),
                nasal_positions: number_array(item.get("nasal")),
                devoice_positions: number_array(item.get("devoice")),
            }
        } else {
            PronunciationKind::PhoneticTranscription {
                ipa: item
                    .get("ipa")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }
        };
        result.push(Pronunciation {
            kind,
            tags: Vec::new(),
            tag_groups,
        });
    }
    result
}

/// _toNumberArray.
fn number_array(value: Option<&Value>) -> Vec<Value> {
    match value {
        Some(Value::Array(items)) => items.clone(),
        Some(n @ Value::Number(_)) => vec![n.clone()],
        _ => Vec::new(),
    }
}

/// Result of [`frequency_info`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrequencyInfo {
    pub frequency: f64,
    pub display_value: Option<String>,
    pub display_value_parsed: bool,
}

/// _getFrequencyInfo: a number, a string (its first number is the value),
/// or `{value, displayValue}`.
pub(crate) fn frequency_info(frequency: &Value) -> FrequencyInfo {
    let mut info = FrequencyInfo {
        frequency: 0.0,
        display_value: None,
        display_value_parsed: false,
    };
    match frequency {
        Value::Object(map) => {
            if let Some(value) = map.get("value").and_then(Value::as_f64) {
                info.frequency = value;
            }
            if let Some(display) = map.get("displayValue").and_then(Value::as_str) {
                info.display_value = Some(display.to_string());
            }
        }
        Value::Number(n) => info.frequency = n.as_f64().unwrap_or(0.0),
        Value::String(s) => {
            info.display_value = Some(s.clone());
            info.display_value_parsed = true;
            info.frequency = string_to_number(s);
        }
        _ => {}
    }
    info
}

/// _convertStringToNumber: the first match of
/// `/[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?/`, parsed, or 0 if none or
/// not finite.
pub(crate) fn string_to_number(text: &str) -> f64 {
    let bytes = text.as_bytes();
    for start in 0..bytes.len() {
        if let Some(end) = match_number(bytes, start) {
            let value: f64 = text
                .get(start..end)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            return if value.is_finite() { value } else { 0.0 };
        }
    }
    0.0
}

/// End of the regex match starting at `start`, if any. The leading sign is
/// optional, so a sign not followed by a number simply fails here (the
/// match is then tried from the next position).
fn match_number(bytes: &[u8], start: usize) -> Option<usize> {
    let digits = |from: usize| {
        bytes.get(from..).map_or(0, |rest| {
            rest.iter().take_while(|b| b.is_ascii_digit()).count()
        })
    };
    let mut i = start;
    if matches!(bytes.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let integer = digits(i);
    if integer > 0 {
        i += integer;
        if bytes.get(i) == Some(&b'.') {
            i += 1 + digits(i + 1);
        }
    } else if bytes.get(i) == Some(&b'.') && digits(i + 1) > 0 {
        i += 1 + digits(i + 1);
    } else {
        return None;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(bytes.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let exponent = digits(j);
        if exponent > 0 {
            i = j + exponent;
        }
    }
    Some(i)
}

/// _updateSortFrequencies: each entry's and definition's frequency order
/// from the frequencies of `dictionary_id`.
pub(crate) fn update_sort_frequencies(
    entries: &mut [TermEntry],
    dictionary_id: i64,
    order: FrequencyOrder,
) {
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    let ascending = order == FrequencyOrder::Ascending;
    let order_of = |min: f64, max: f64| {
        if min <= max {
            if ascending { min } else { -max }
        } else if ascending {
            MAX_SAFE_INTEGER
        } else {
            0.0
        }
    };
    let mut by_headword: HashMap<usize, f64> = HashMap::new();
    for entry in entries {
        by_headword.clear();
        let mut min = MAX_SAFE_INTEGER;
        let mut max = -MAX_SAFE_INTEGER;
        for item in &entry.frequencies {
            if item.dictionary_id != dictionary_id {
                continue;
            }
            by_headword.insert(item.headword_index, item.frequency);
            min = min.min(item.frequency);
            max = max.max(item.frequency);
        }
        entry.frequency_order = order_of(min, max);
        for definition in &mut entry.definitions {
            let mut min = MAX_SAFE_INTEGER;
            let mut max = -MAX_SAFE_INTEGER;
            for headword_index in &definition.headword_indices {
                if let Some(&frequency) = by_headword.get(headword_index) {
                    min = min.min(frequency);
                    max = max.max(frequency);
                }
            }
            definition.frequency_order = order_of(min, max);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_frequencies_parse_like_yomitan() {
        let cases = [
            ("five (5)", 5.0),
            ("four", 0.0),
            ("twenty-one (21)", 21.0),
            ("-3x", -3.0),
            ("+.5", 0.5),
            ("a+b 7", 7.0),
            ("1.e3", 1000.0),
            ("2e", 2.0),
            ("1e999", 0.0),
            ("x.y", 0.0),
            ("..7", 0.7),
            ("12/34", 12.0),
        ];
        for (text, expected) in cases {
            assert_eq!(string_to_number(text), expected, "{text}");
        }
    }
}
