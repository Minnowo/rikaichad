// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kanji_meta_bank_N.json` rows: kanji frequency data.

use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde_json::Value;

use super::term_meta::validate_frequency_value;
use super::{check_key, end, field};
use crate::limits::Limits;

const ARITY: usize = 3;

/// One kanji metadata entry: `[character, "freq", frequency]`.
#[derive(Debug, Clone, PartialEq)]
pub struct KanjiMetaRow {
    pub character: String,
    /// Always `"freq"` (the only mode the schema allows).
    pub mode: String,
    pub data: Value,
}

impl KanjiMetaRow {
    /// Checks index-key sizes against `limits`.
    pub fn check(&self, limits: &Limits) -> Result<(), String> {
        if self.character.is_empty() {
            return Err("kanji character is empty".to_string());
        }
        check_key(&self.character, "character", limits.max_key_bytes)
    }
}

impl<'de> Deserialize<'de> for KanjiMetaRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(KanjiMetaRowVisitor)
    }
}

struct KanjiMetaRowVisitor;

impl<'de> Visitor<'de> for KanjiMetaRowVisitor {
    type Value = KanjiMetaRow;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a kanji meta array of {ARITY} fields")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<KanjiMetaRow, A::Error> {
        let character = field(&mut seq, 0, "character", ARITY)?;
        let mode: String = field(&mut seq, 1, "mode", ARITY)?;
        let data: Value = field(&mut seq, 2, "data", ARITY)?;
        end(&mut seq, ARITY)?;
        if mode != "freq" {
            return Err(de::Error::custom(format!(
                "field 1 (mode): must be \"freq\", got {mode:?}"
            )));
        }
        validate_frequency_value(&data)
            .map_err(|e| de::Error::custom(format!("field 2 (data): {e}")))?;
        Ok(KanjiMetaRow {
            character,
            mode,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates() {
        let row: KanjiMetaRow =
            serde_json::from_str(r#"["食","freq",{"value":328,"displayValue":"328"}]"#).unwrap();
        assert_eq!(row.mode, "freq");
        assert!(serde_json::from_str::<KanjiMetaRow>(r#"["食","pitch",1]"#).is_err());
    }
}
