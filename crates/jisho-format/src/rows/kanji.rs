// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kanji_bank_N.json` rows (format 3).

use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};

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
    /// Stat name (a tag name) and value pairs, in the order JavaScript's
    /// `Object.entries` gives them (see [`KanjiStats`]).
    pub stats: KanjiStats,
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

/// A kanji's stats object as ordered pairs. Yomitan groups stats in the
/// order of `Object.entries`: integer-like keys first in ascending numeric
/// order, then the other keys in source order; a repeated key keeps its
/// first position and its last value (as `JSON.parse` does).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KanjiStats(pub Vec<(String, String)>);

impl KanjiStats {
    /// The value of stat `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

impl<'de> Deserialize<'de> for KanjiStats {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(KanjiStatsVisitor)
    }
}

struct KanjiStatsVisitor;

impl<'de> Visitor<'de> for KanjiStatsVisitor {
    type Value = KanjiStats;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "an object of string values")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<KanjiStats, A::Error> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut positions: HashMap<String, usize> = HashMap::new();
        while let Some((key, value)) = map.next_entry::<String, String>()? {
            match positions.get(&key).and_then(|&i| pairs.get_mut(i)) {
                Some(pair) => pair.1 = value,
                None => {
                    positions.insert(key.clone(), pairs.len());
                    pairs.push((key, value));
                }
            }
        }
        // Stable: other keys keep their source order.
        pairs.sort_by_key(|(k, _)| array_index(k).map_or((1, 0), |i| (0, i)));
        Ok(KanjiStats(pairs))
    }
}

/// The value of `key` if JavaScript treats it as an array index (a
/// canonical decimal integer below 2^32 - 1), which `Object.entries` lists
/// first.
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key.starts_with('0')) {
        return None;
    }
    if !key.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    key.parse::<u32>().ok().filter(|&i| i < u32::MAX)
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
        assert_eq!(row.stats.get("strokes"), Some("9"));
        let err = serde_json::from_str::<KanjiRow>(r#"["x","","","",[],{"strokes":9}]"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("field 5 (stats)"), "{err}");
    }

    #[test]
    fn stats_keep_javascript_entry_order() {
        let row: KanjiRow = serde_json::from_str(
            r#"["x","","","",[],{"b":"1","10":"2","a":"3","2":"4","b":"5","01":"6","4294967295":"7"}]"#,
        )
        .unwrap();
        let keys: Vec<(&str, &str)> = row
            .stats
            .0
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("2", "4"),
                ("10", "2"),
                ("b", "5"),
                ("a", "3"),
                ("01", "6"),
                ("4294967295", "7")
            ]
        );
    }
}
