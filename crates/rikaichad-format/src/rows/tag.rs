// SPDX-License-Identifier: AGPL-3.0-or-later
//! `tag_bank_N.json` rows.

use std::fmt;

use serde::Deserialize;
use serde::de::{Deserializer, SeqAccess, Visitor};

use super::{check_key, end, field};
use crate::limits::Limits;

const ARITY: usize = 5;

/// One tag definition: `[name, category, order, notes, score]`.
#[derive(Debug, Clone, PartialEq)]
pub struct TagRow {
    pub name: String,
    /// Category, e.g. `partOfSpeech`, `popular`; drives display colour.
    pub category: String,
    /// Sort order among tags; lower first.
    pub order: f64,
    /// Human-readable description.
    pub notes: String,
    /// Score contribution of terms carrying this tag.
    pub score: f64,
}

impl TagRow {
    /// Checks index-key sizes against `limits`.
    pub fn check(&self, limits: &Limits) -> Result<(), String> {
        check_key(&self.name, "tag name", limits.max_key_bytes)
    }
}

impl<'de> Deserialize<'de> for TagRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(TagRowVisitor)
    }
}

struct TagRowVisitor;

impl<'de> Visitor<'de> for TagRowVisitor {
    type Value = TagRow;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a tag array of {ARITY} fields")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<TagRow, A::Error> {
        let row = TagRow {
            name: field(&mut seq, 0, "name", ARITY)?,
            category: field(&mut seq, 1, "category", ARITY)?,
            order: field(&mut seq, 2, "order", ARITY)?,
            notes: field(&mut seq, 3, "notes", ARITY)?,
            score: field(&mut seq, 4, "score", ARITY)?,
        };
        end(&mut seq, ARITY)?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tag() {
        let row: TagRow = serde_json::from_str(r#"["P","popular",-10,"common word",10]"#).unwrap();
        assert_eq!(row.name, "P");
        assert_eq!(row.order, -10.0);
    }
}
