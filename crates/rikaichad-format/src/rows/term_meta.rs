// SPDX-License-Identifier: AGPL-3.0-or-later
//! `term_meta_bank_N.json` rows: frequency, pitch accent and IPA data.

use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde_json::Value;

use super::{check_key, end, field};
use crate::limits::Limits;

const ARITY: usize = 3;

/// One term metadata entry: `[term, mode, data]`, where mode is `freq`,
/// `pitch` or `ipa`. `data` is validated against the mode's shape in
/// `docs/reference/yomitan-schemas/dictionary-term-meta-bank-v3-schema.json`
/// and kept as JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct TermMetaRow {
    pub term: String,
    pub mode: String,
    pub data: Value,
}

impl TermMetaRow {
    /// Checks index-key sizes against `limits`.
    pub fn check(&self, limits: &Limits) -> Result<(), String> {
        check_key(&self.term, "term", limits.max_key_bytes)
    }
}

impl<'de> Deserialize<'de> for TermMetaRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(TermMetaRowVisitor)
    }
}

struct TermMetaRowVisitor;

impl<'de> Visitor<'de> for TermMetaRowVisitor {
    type Value = TermMetaRow;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a term meta array of {ARITY} fields")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<TermMetaRow, A::Error> {
        let term = field(&mut seq, 0, "term", ARITY)?;
        let mode: String = field(&mut seq, 1, "mode", ARITY)?;
        let data: Value = field(&mut seq, 2, "data", ARITY)?;
        end(&mut seq, ARITY)?;
        let valid = match mode.as_str() {
            "freq" => validate_freq_data(&data),
            "pitch" => validate_pitch_data(&data),
            "ipa" => validate_ipa_data(&data),
            other => Err(format!(
                "mode must be \"freq\", \"pitch\" or \"ipa\", got {other:?}"
            )),
        };
        valid.map_err(|e| de::Error::custom(format!("field 2 (data): {e}")))?;
        Ok(TermMetaRow { term, mode, data })
    }
}

/// Frequency data: a frequency value, or `{reading, frequency}`.
fn validate_freq_data(data: &Value) -> Result<(), String> {
    if let Value::Object(map) = data
        && map.contains_key("reading")
    {
        only_keys(map, &["reading", "frequency"])?;
        string_at(map, "reading")?;
        let frequency = map
            .get("frequency")
            .ok_or_else(|| "missing \"frequency\"".to_string())?;
        return validate_frequency_value(frequency);
    }
    validate_frequency_value(data)
}

/// A frequency value: number, string, or `{value, displayValue?}`.
/// Also used for kanji frequencies.
pub(crate) fn validate_frequency_value(value: &Value) -> Result<(), String> {
    match value {
        Value::Number(_) | Value::String(_) => Ok(()),
        Value::Object(map) => {
            only_keys(map, &["value", "displayValue"])?;
            match map.get("value") {
                Some(Value::Number(_)) => {}
                _ => return Err("frequency \"value\" must be a number".to_string()),
            }
            match map.get("displayValue") {
                None | Some(Value::String(_)) => Ok(()),
                Some(_) => Err("frequency \"displayValue\" must be a string".to_string()),
            }
        }
        _ => Err("frequency must be a number, string or {value, displayValue}".to_string()),
    }
}

/// Pitch data: `{reading, pitches: [{position, nasal?, devoice?, tags?}]}`.
fn validate_pitch_data(data: &Value) -> Result<(), String> {
    let Value::Object(map) = data else {
        return Err("pitch data must be an object".to_string());
    };
    only_keys(map, &["reading", "pitches"])?;
    string_at(map, "reading")?;
    let Some(Value::Array(pitches)) = map.get("pitches") else {
        return Err("\"pitches\" must be an array".to_string());
    };
    for pitch in pitches {
        let Value::Object(pitch) = pitch else {
            return Err("each pitch must be an object".to_string());
        };
        only_keys(pitch, &["position", "nasal", "devoice", "tags"])?;
        match pitch.get("position") {
            Some(n @ Value::Number(_)) if is_non_negative_integer(n) => {}
            Some(Value::String(s))
                if !s.is_empty() && s.bytes().all(|b| b == b'H' || b == b'L') => {}
            _ => {
                return Err(
                    "pitch \"position\" must be a non-negative integer or an H/L string"
                        .to_string(),
                );
            }
        }
        for key in ["nasal", "devoice"] {
            match pitch.get(key) {
                None => {}
                Some(n @ Value::Number(_)) if is_non_negative_integer(n) => {}
                Some(Value::Array(items)) if items.iter().all(is_non_negative_integer) => {}
                Some(_) => {
                    return Err(format!(
                        "pitch \"{key}\" must be a non-negative integer or an array of them"
                    ));
                }
            }
        }
        string_array_at(pitch, "tags")?;
    }
    Ok(())
}

/// IPA data: `{reading, transcriptions: [{ipa, tags?}]}`.
fn validate_ipa_data(data: &Value) -> Result<(), String> {
    let Value::Object(map) = data else {
        return Err("ipa data must be an object".to_string());
    };
    only_keys(map, &["reading", "transcriptions"])?;
    string_at(map, "reading")?;
    let Some(Value::Array(items)) = map.get("transcriptions") else {
        return Err("\"transcriptions\" must be an array".to_string());
    };
    for item in items {
        let Value::Object(item) = item else {
            return Err("each transcription must be an object".to_string());
        };
        only_keys(item, &["ipa", "tags"])?;
        string_at(item, "ipa")?;
        string_array_at(item, "tags")?;
    }
    Ok(())
}

/// JSON Schema "integer" with `minimum: 0`: integral numbers, including
/// forms like `2.0`, as for term sequence numbers.
fn is_non_negative_integer(value: &Value) -> bool {
    match value {
        Value::Number(n) => {
            n.as_u64().is_some()
                || n.as_f64()
                    .is_some_and(|f| f >= 0.0 && f.fract() == 0.0 && f <= 9.0e15)
        }
        _ => false,
    }
}

fn only_keys(map: &serde_json::Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    match map.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(key) => Err(format!("unexpected key {key:?}")),
        None => Ok(()),
    }
}

fn string_at(map: &serde_json::Map<String, Value>, key: &str) -> Result<(), String> {
    match map.get(key) {
        Some(Value::String(_)) => Ok(()),
        _ => Err(format!("\"{key}\" must be a string")),
    }
}

fn string_array_at(map: &serde_json::Map<String, Value>, key: &str) -> Result<(), String> {
    match map.get(key) {
        None => Ok(()),
        Some(Value::Array(items)) if items.iter().all(Value::is_string) => Ok(()),
        Some(_) => Err(format!("\"{key}\" must be an array of strings")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<TermMetaRow, String> {
        serde_json::from_str::<TermMetaRow>(text).map_err(|e| e.to_string())
    }

    #[test]
    fn accepts_every_frequency_shape_seen_in_real_dictionaries() {
        for data in [
            "5",
            "\"5/10\"",
            r#"{"value":5,"displayValue":"5"}"#,
            r#"{"value":5}"#,
            r#"{"reading":"x","frequency":5}"#,
            r#"{"reading":"x","frequency":{"value":5,"displayValue":"5"}}"#,
        ] {
            parse(&format!(r#"["t","freq",{data}]"#)).unwrap();
        }
    }

    #[test]
    fn accepts_pitch_and_ipa_shapes() {
        parse(r#"["t","pitch",{"reading":"x","pitches":[{"position":0,"nasal":[],"devoice":[2]},{"position":"LHH","tags":["n"]}]}]"#).unwrap();
        // JSON Schema "integer" accepts integral floats, as for sequences.
        parse(r#"["t","pitch",{"reading":"x","pitches":[{"position":2.0,"nasal":1.0,"devoice":[3.0]}]}]"#).unwrap();
        parse(r#"["t","ipa",{"reading":"x","transcriptions":[{"ipa":"a","tags":["t"]}]}]"#)
            .unwrap();
    }

    #[test]
    fn rejects_bad_mode_and_shapes() {
        for text in [
            r#"["t","bogus",1]"#,
            r#"["t","freq",{"value":"x"}]"#,
            r#"["t","freq",{"value":1,"extra":2}]"#,
            r#"["t","freq",true]"#,
            r#"["t","pitch",{"reading":"x","pitches":[{"position":-1}]}]"#,
            r#"["t","pitch",{"reading":"x","pitches":[{"position":1.5}]}]"#,
            r#"["t","pitch",{"reading":"x","pitches":[{"position":"HX"}]}]"#,
            r#"["t","pitch",{"pitches":[]}]"#,
            r#"["t","ipa",{"reading":"x","transcriptions":[{"tags":[]}]}]"#,
        ] {
            let err = parse(text).unwrap_err();
            assert!(err.contains("field"), "{text}: {err}");
        }
    }
}
