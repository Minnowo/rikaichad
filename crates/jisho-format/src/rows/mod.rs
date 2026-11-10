// SPDX-License-Identifier: AGPL-3.0-or-later
//! Row types of the five bank files, each deserialized from a fixed-length
//! JSON array, plus shared helpers for reading array fields.

mod kanji;
mod kanji_meta;
mod tag;
mod term;
mod term_meta;

pub use kanji::KanjiRow;
pub use kanji_meta::KanjiMetaRow;
pub use tag::TagRow;
pub use term::TermRow;
pub use term_meta::TermMetaRow;

use serde::Deserialize;
use serde::de::{self, IgnoredAny, SeqAccess};

/// Reads field `index` (named `field`) of a row that must have `arity`
/// fields, turning a missing field or a type error into a message that
/// names the field.
fn field<'de, A, T>(seq: &mut A, index: usize, field: &str, arity: usize) -> Result<T, A::Error>
where
    A: SeqAccess<'de>,
    T: Deserialize<'de>,
{
    match seq.next_element::<T>() {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(de::Error::custom(format!(
            "row has {index} fields, expected {arity}"
        ))),
        Err(e) => Err(de::Error::custom(format!("field {index} ({field}): {e}"))),
    }
}

/// Fails if the row has more than `arity` fields.
fn end<'de, A: SeqAccess<'de>>(seq: &mut A, arity: usize) -> Result<(), A::Error> {
    match seq.next_element::<IgnoredAny>()? {
        None => Ok(()),
        Some(_) => Err(de::Error::custom(format!(
            "row has more than {arity} fields"
        ))),
    }
}

/// Checks that an index key (term, reading, character, tag name) is within
/// `max_bytes`. Returns a message naming `what` on failure.
fn check_key(value: &str, what: &str, max_bytes: usize) -> Result<(), String> {
    if value.len() > max_bytes {
        return Err(format!(
            "{what} is {} bytes, over the limit of {max_bytes}",
            value.len()
        ));
    }
    Ok(())
}
