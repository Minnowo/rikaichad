// SPDX-License-Identifier: AGPL-3.0-or-later
//! `term_bank_N.json` rows (format 3).

use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde_json::Value;

use super::{check_key, end, field};
use crate::limits::Limits;

const ARITY: usize = 8;

/// One term entry: `[term, reading, definitionTags, rules, score, glossary,
/// sequence, termTags]`.
///
/// Stored as read, except that `definitionTags: null` becomes `""`. The
/// empty-reading rule (reading = term) and glossary normalization are
/// applied by the importer, matching Yomitan's order of operations.
#[derive(Debug, Clone, PartialEq)]
pub struct TermRow {
    pub term: String,
    pub reading: String,
    /// Space-separated tag names for the definitions.
    pub definition_tags: String,
    /// Space-separated deinflection condition ids (e.g. `v1`, `adj-i`).
    pub rules: String,
    /// Popularity score; higher sorts first.
    pub score: f64,
    /// Glossary items as JSON (see [`crate::normalize_glossary`]).
    pub glossary: Vec<Value>,
    /// Sequence number grouping related entries (`-1` style values allowed).
    pub sequence: i64,
    /// Space-separated tag names for the term.
    pub term_tags: String,
}

impl TermRow {
    /// Checks index-key sizes against `limits`.
    pub fn check(&self, limits: &Limits) -> Result<(), String> {
        check_key(&self.term, "term", limits.max_key_bytes)?;
        check_key(&self.reading, "reading", limits.max_key_bytes)
    }
}

impl<'de> Deserialize<'de> for TermRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(TermRowVisitor)
    }
}

struct TermRowVisitor;

impl<'de> Visitor<'de> for TermRowVisitor {
    type Value = TermRow;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a term array of {ARITY} fields")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<TermRow, A::Error> {
        let term = field(&mut seq, 0, "term", ARITY)?;
        let reading = field(&mut seq, 1, "reading", ARITY)?;
        let definition_tags: Option<String> = field(&mut seq, 2, "definitionTags", ARITY)?;
        let rules = field(&mut seq, 3, "rules", ARITY)?;
        let score = field(&mut seq, 4, "score", ARITY)?;
        let glossary = field(&mut seq, 5, "glossary", ARITY)?;
        let sequence: Integer = field(&mut seq, 6, "sequence", ARITY)?;
        let term_tags = field(&mut seq, 7, "termTags", ARITY)?;
        end(&mut seq, ARITY)?;
        Ok(TermRow {
            term,
            reading,
            definition_tags: definition_tags.unwrap_or_default(),
            rules,
            score,
            glossary,
            sequence: sequence.0,
            term_tags,
        })
    }
}

/// A JSON Schema "integer": an integral number, including forms like `5.0`.
struct Integer(i64);

impl<'de> Deserialize<'de> for Integer {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        if value.fract() != 0.0 || value.abs() > 9.0e15 {
            return Err(de::Error::custom(format!(
                "expected an integer, got {value}"
            )));
        }
        Ok(Integer(value as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_row_and_null_definition_tags() {
        let row: TermRow =
            serde_json::from_str(r#"["食べる","たべる",null,"v1",5,["to eat"],1358280,"P"]"#)
                .unwrap();
        assert_eq!(row.term, "食べる");
        assert_eq!(row.definition_tags, "");
        assert_eq!(row.rules, "v1");
        assert_eq!(row.score, 5.0);
        assert_eq!(row.glossary, vec![Value::from("to eat")]);
        assert_eq!(row.sequence, 1358280);
    }

    #[test]
    fn reports_wrong_arity_and_field_types() {
        let short = serde_json::from_str::<TermRow>(r#"["a","b","","",0,[],1]"#).unwrap_err();
        assert!(
            short.to_string().contains("row has 7 fields, expected 8"),
            "{short}"
        );
        let long = serde_json::from_str::<TermRow>(r#"["a","b","","",0,[],1,"",9]"#).unwrap_err();
        assert!(long.to_string().contains("more than 8 fields"), "{long}");
        let bad = serde_json::from_str::<TermRow>(r#"["a","b","","","x",[],1,""]"#).unwrap_err();
        assert!(bad.to_string().contains("field 4 (score)"), "{bad}");
        let frac = serde_json::from_str::<TermRow>(r#"["a","b","","",0,[],1.5,""]"#).unwrap_err();
        assert!(frac.to_string().contains("field 6 (sequence)"), "{frac}");
    }
}
