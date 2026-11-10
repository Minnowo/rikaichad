// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kanji_bank_N.json` rows (format 3).

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{Deserializer, SeqAccess, Visitor};

use super::{check_key, end, field};
use crate::limits::Limits;

const ARITY: usize = 6;

/// One kanji entry: `[character, onyomi, kunyomi, tags, meanings, stats]`.
#[derive(Debug, Clone, PartialEq)]
pub struct KanjiRow {
    pub character: String,
    /// Space-separated on readings.
    pub onyomi: String,
    /// Space-separated kun readings.
    pub kunyomi: String,
    /// Space-separated tag names.
    pub tags: String,
    pub meanings: Vec<String>,
    /// Stat name to value (both strings), keyed by tag name.
    pub stats: BTreeMap<String, String>,
}

impl KanjiRow {
    /// Checks index-key sizes against `limits`; the character must not be
    /// empty (schema `minLength: 1`).
    pub fn check(&self, limits: &Limits) -> Result<(), String> {
        if self.character.is_empty() {
            return Err("kanji character is empty".to_string());
        }
        check_key(&self.character, "character", limits.max_key_bytes)
    }
}

impl<'de> Deserialize<'de> for KanjiRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(KanjiRowVisitor)
    }
}

struct KanjiRowVisitor;

impl<'de> Visitor<'de> for KanjiRowVisitor {
    type Value = KanjiRow;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a kanji array of {ARITY} fields")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<KanjiRow, A::Error> {
        let row = KanjiRow {
            character: field(&mut seq, 0, "character", ARITY)?,
            onyomi: field(&mut seq, 1, "onyomi", ARITY)?,
            kunyomi: field(&mut seq, 2, "kunyomi", ARITY)?,
            tags: field(&mut seq, 3, "tags", ARITY)?,
            meanings: field(&mut seq, 4, "meanings", ARITY)?,
            stats: field(&mut seq, 5, "stats", ARITY)?,
        };
        end(&mut seq, ARITY)?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kanji_row() {
        let row: KanjiRow = serde_json::from_str(
            r#"["食","ショク ジキ","く.う た.べる","jouyou",["eat","food"],{"strokes":"9"}]"#,
        )
        .unwrap();
        assert_eq!(row.meanings, vec!["eat", "food"]);
        assert_eq!(row.stats.get("strokes").map(String::as_str), Some("9"));
        let err = serde_json::from_str::<KanjiRow>(r#"["x","","","",[],{"strokes":9}]"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("field 5 (stats)"), "{err}");
    }
}
