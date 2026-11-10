// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parsing and validation of `index.json`, the dictionary metadata file.
//!
//! Rules follow `docs/reference/yomitan-schemas/dictionary-index-schema.json`
//! and Yomitan's importer: title and revision are required and non-empty,
//! `format` (or its alias `version`) gives the format, unknown keys are
//! allowed. jisho additionally accepts only format 3 and Japanese source
//! dictionaries (brief section 4, items 16 and 21).

use serde_json::{Map, Value};

use crate::error::FormatError;

/// The only dictionary format jisho reads.
pub const SUPPORTED_FORMAT: i64 = 3;

/// Validated contents of `index.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    pub title: String,
    pub revision: String,
    pub format: i64,
    pub sequenced: bool,
    pub minimum_yomitan_version: Option<String>,
    pub author: Option<String>,
    pub url: Option<String>,
    pub description: Option<String>,
    pub attribution: Option<String>,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub frequency_mode: Option<FrequencyMode>,
    pub is_updatable: bool,
    pub index_url: Option<String>,
    pub download_url: Option<String>,
    /// Tags from the obsolete `tagMeta` object, sorted by name. Yomitan
    /// adds these to the tag list of the dictionary (in object order, which
    /// only affects row ids, never results).
    pub tag_meta: Vec<IndexTag>,
}

/// How a frequency dictionary's numbers are meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrequencyMode {
    /// Higher number = more frequent (raw counts).
    OccurrenceBased,
    /// Lower number = more frequent (rank 1 is the most frequent).
    RankBased,
}

impl FrequencyMode {
    /// The spelling used in `index.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            FrequencyMode::OccurrenceBased => "occurrence-based",
            FrequencyMode::RankBased => "rank-based",
        }
    }
}

/// One tag from the obsolete `index.json` `tagMeta` object.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexTag {
    pub name: String,
    pub category: String,
    pub order: f64,
    pub notes: String,
    pub score: f64,
}

impl Index {
    /// Parses and validates the text of `index.json`.
    ///
    /// Errors: invalid JSON, a required field missing or empty, a field of
    /// the wrong type, an unsupported format, or a source language other
    /// than Japanese.
    pub fn parse(text: &str) -> Result<Index, FormatError> {
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| invalid(&format!("not valid JSON: {e}")))?;
        let Value::Object(map) = value else {
            return Err(invalid("must be a JSON object"));
        };

        let title = required_string(&map, "title")?;
        let revision = required_string(&map, "revision")?;
        let format = match (map.get("format"), map.get("version")) {
            (Some(v), _) => integer(v, "format")?,
            (None, Some(v)) => integer(v, "version")?,
            (None, None) => return Err(invalid("\"format\" is required")),
        };
        if !(1..=3).contains(&format) {
            return Err(invalid(&format!(
                "\"format\" must be 1, 2 or 3, got {format}"
            )));
        }
        if format != SUPPORTED_FORMAT {
            return Err(FormatError::UnsupportedFormat { version: format });
        }

        let source_language = optional_string(&map, "sourceLanguage")?;
        let target_language = optional_string(&map, "targetLanguage")?;
        for (key, lang) in [
            ("sourceLanguage", &source_language),
            ("targetLanguage", &target_language),
        ] {
            if let Some(code) = lang {
                let valid =
                    (2..=3).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase());
                if !valid {
                    return Err(invalid(&format!(
                        "\"{key}\" must be an ISO 639 code, got {code:?}"
                    )));
                }
            }
        }
        if let Some(code) = &source_language
            && code != "ja"
        {
            return Err(invalid(&format!(
                "source language {code:?} is not supported (only \"ja\")"
            )));
        }

        let frequency_mode = match optional_string(&map, "frequencyMode")?.as_deref() {
            None => None,
            Some("occurrence-based") => Some(FrequencyMode::OccurrenceBased),
            Some("rank-based") => Some(FrequencyMode::RankBased),
            Some(other) => {
                return Err(invalid(&format!(
                    "\"frequencyMode\" must be \"occurrence-based\" or \"rank-based\", got {other:?}"
                )));
            }
        };

        let is_updatable = match map.get("isUpdatable") {
            None => false,
            Some(Value::Bool(true)) => true,
            Some(_) => return Err(invalid("\"isUpdatable\" must be true when present")),
        };
        let index_url = optional_string(&map, "indexUrl")?;
        let download_url = optional_string(&map, "downloadUrl")?;
        if is_updatable && (index_url.is_none() || download_url.is_none()) {
            return Err(invalid(
                "\"isUpdatable\" requires \"indexUrl\" and \"downloadUrl\"",
            ));
        }

        let sequenced = match map.get("sequenced") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(invalid("\"sequenced\" must be a boolean")),
        };

        Ok(Index {
            title,
            revision,
            format,
            sequenced,
            minimum_yomitan_version: optional_string(&map, "minimumYomitanVersion")?,
            author: optional_string(&map, "author")?,
            url: optional_string(&map, "url")?,
            description: optional_string(&map, "description")?,
            attribution: optional_string(&map, "attribution")?,
            source_language,
            target_language,
            frequency_mode,
            is_updatable,
            index_url,
            download_url,
            tag_meta: tag_meta(&map)?,
        })
    }
}

fn tag_meta(map: &Map<String, Value>) -> Result<Vec<IndexTag>, FormatError> {
    let Some(value) = map.get("tagMeta") else {
        return Ok(Vec::new());
    };
    let Value::Object(tags) = value else {
        return Err(invalid("\"tagMeta\" must be an object"));
    };
    let mut result = Vec::with_capacity(tags.len());
    for (name, info) in tags {
        let Value::Object(info) = info else {
            return Err(invalid(&format!("\"tagMeta\".{name:?} must be an object")));
        };
        let field = |key: &str| format!("tagMeta.{name}.{key}");
        result.push(IndexTag {
            name: name.clone(),
            category: optional_string_at(info, "category", &field("category"))?.unwrap_or_default(),
            order: optional_number_at(info, "order", &field("order"))?.unwrap_or(0.0),
            notes: optional_string_at(info, "notes", &field("notes"))?.unwrap_or_default(),
            score: optional_number_at(info, "score", &field("score"))?.unwrap_or(0.0),
        });
    }
    Ok(result)
}

fn required_string(map: &Map<String, Value>, key: &str) -> Result<String, FormatError> {
    match optional_string(map, key)? {
        Some(s) if !s.is_empty() => Ok(s),
        Some(_) => Err(invalid(&format!("\"{key}\" must not be empty"))),
        None => Err(invalid(&format!("\"{key}\" is required"))),
    }
}

fn optional_string(map: &Map<String, Value>, key: &str) -> Result<Option<String>, FormatError> {
    optional_string_at(map, key, key)
}

fn optional_string_at(
    map: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<String>, FormatError> {
    match map.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(invalid(&format!("\"{path}\" must be a string"))),
    }
}

fn optional_number_at(
    map: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<f64>, FormatError> {
    match map.get(key) {
        None => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(_) => Err(invalid(&format!("\"{path}\" must be a number"))),
    }
}

/// Accepts JSON integers, including integral floats such as `3.0`, as JSON
/// Schema's "integer" does.
fn integer(value: &Value, key: &str) -> Result<i64, FormatError> {
    let Value::Number(n) = value else {
        return Err(invalid(&format!("\"{key}\" must be an integer")));
    };
    if let Some(i) = n.as_i64() {
        return Ok(i);
    }
    match n.as_f64() {
        Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => Ok(f as i64),
        _ => Err(invalid(&format!("\"{key}\" must be an integer"))),
    }
}

fn invalid(message: &str) -> FormatError {
    FormatError::InvalidIndex {
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_index() {
        let index = Index::parse(r#"{"title":"T","revision":"r1","format":3}"#).unwrap();
        assert_eq!(index.title, "T");
        assert_eq!(index.revision, "r1");
        assert_eq!(index.format, 3);
        assert!(!index.sequenced);
        assert!(index.tag_meta.is_empty());
    }

    #[test]
    fn accepts_version_alias_bom_and_unknown_keys() {
        let index =
            Index::parse("\u{feff}{\"title\":\"T\",\"revision\":\"r\",\"version\":3,\"weight\":5}")
                .unwrap();
        assert_eq!(index.format, 3);
    }

    #[test]
    fn format_wins_over_version() {
        let err =
            Index::parse(r#"{"title":"T","revision":"r","format":1,"version":3}"#).unwrap_err();
        assert!(matches!(err, FormatError::UnsupportedFormat { version: 1 }));
    }

    #[test]
    fn rejects_missing_or_empty_required_fields() {
        for text in [
            r#"{"revision":"r","format":3}"#,
            r#"{"title":"","revision":"r","format":3}"#,
            r#"{"title":"T","format":3}"#,
            r#"{"title":"T","revision":"r"}"#,
            r#"{"title":"T","revision":"r","format":4}"#,
            r#"{"title":"T","revision":"r","format":"3"}"#,
            r#"[]"#,
        ] {
            assert!(Index::parse(text).is_err(), "accepted {text}");
        }
    }

    #[test]
    fn validates_languages_and_updatable() {
        let base = r#""title":"T","revision":"r","format":3"#;
        assert!(Index::parse(&format!("{{{base},\"sourceLanguage\":\"en\"}}")).is_err());
        assert!(
            Index::parse(&format!(
                "{{{base},\"sourceLanguage\":\"ja\",\"targetLanguage\":\"en\"}}"
            ))
            .is_ok()
        );
        assert!(Index::parse(&format!("{{{base},\"isUpdatable\":true}}")).is_err());
        assert!(Index::parse(&format!("{{{base},\"isUpdatable\":false}}")).is_err());
        let ok = Index::parse(&format!(
            "{{{base},\"isUpdatable\":true,\"indexUrl\":\"u\",\"downloadUrl\":\"d\"}}"
        ))
        .unwrap();
        assert!(ok.is_updatable);
    }

    #[test]
    fn reads_frequency_mode_and_tag_meta() {
        let index = Index::parse(
            r#"{"title":"T","revision":"r","format":3,"frequencyMode":"rank-based",
                "tagMeta":{"P":{"category":"popular","order":-10,"notes":"common","score":10}}}"#,
        )
        .unwrap();
        assert_eq!(index.frequency_mode, Some(FrequencyMode::RankBased));
        assert_eq!(
            index.tag_meta,
            vec![IndexTag {
                name: "P".to_string(),
                category: "popular".to_string(),
                order: -10.0,
                notes: "common".to_string(),
                score: 10.0,
            }]
        );
    }
}
