// SPDX-License-Identifier: AGPL-3.0-or-later
//! Streaming parse of bank files: one row at a time, never the whole file.
//!
//! Bank files can be over 100 MB of JSON (docs/dictionary-survey.md), so the
//! top-level array is visited element by element and each row is handed to
//! a callback, keeping memory bounded by one row.

use std::fmt;
use std::io::{BufRead, BufReader, Read};

use serde::Deserializer as _;
use serde::de::{self, DeserializeOwned, SeqAccess, Visitor};

use crate::error::FormatError;

const BUFFER_BYTES: usize = 256 * 1024;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

/// Parses the JSON array in `reader` and calls `on_row` for each element,
/// in order. `name` is the archive entry name, used in error messages.
///
/// Returns the number of rows. Stops at the first error: invalid JSON, a
/// row that does not deserialize as `T` (the message names the row and
/// field), trailing data after the array, or an error returned by
/// `on_row`, which is passed through unchanged.
pub fn stream_rows<T, F>(reader: &mut dyn Read, name: &str, on_row: F) -> Result<u64, FormatError>
where
    T: DeserializeOwned,
    F: FnMut(T) -> Result<(), FormatError>,
{
    let mut buffered = BufReader::with_capacity(BUFFER_BYTES, reader);
    skip_bom(&mut buffered).map_err(|e| json_error(name, &e.to_string()))?;
    let mut deserializer = serde_json::Deserializer::from_reader(buffered);
    let mut aborted = None;
    let visitor = RowsVisitor {
        on_row,
        aborted: &mut aborted,
        marker: std::marker::PhantomData,
    };
    let result = deserializer.deserialize_seq(visitor);
    if let Some(err) = aborted {
        return Err(err);
    }
    let count = result.map_err(|e| json_error(name, &e.to_string()))?;
    deserializer
        .end()
        .map_err(|e| json_error(name, &format!("data after the top-level array: {e}")))?;
    Ok(count)
}

struct RowsVisitor<'a, T, F> {
    on_row: F,
    /// Set when `on_row` fails, so its error survives the trip through
    /// serde's error type.
    aborted: &'a mut Option<FormatError>,
    marker: std::marker::PhantomData<T>,
}

impl<'de, T, F> Visitor<'de> for RowsVisitor<'_, T, F>
where
    T: DeserializeOwned,
    F: FnMut(T) -> Result<(), FormatError>,
{
    type Value = u64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a JSON array of rows")
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<u64, A::Error> {
        let mut count: u64 = 0;
        loop {
            let row = match seq.next_element::<T>() {
                Ok(Some(row)) => row,
                Ok(None) => return Ok(count),
                Err(e) => return Err(de::Error::custom(format!("row {count}: {e}"))),
            };
            if let Err(err) = (self.on_row)(row) {
                *self.aborted = Some(err);
                return Err(de::Error::custom("aborted by row handler"));
            }
            count += 1;
        }
    }
}

fn skip_bom(reader: &mut impl BufRead) -> std::io::Result<()> {
    let head = reader.fill_buf()?;
    if head.starts_with(UTF8_BOM) {
        reader.consume(UTF8_BOM.len());
    }
    Ok(())
}

fn json_error(name: &str, message: &str) -> FormatError {
    FormatError::Json {
        name: name.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::TagRow;

    fn collect(text: &[u8]) -> Result<Vec<TagRow>, FormatError> {
        let mut rows = Vec::new();
        let mut reader: &[u8] = text;
        stream_rows(&mut reader, "tag_bank_1.json", |row: TagRow| {
            rows.push(row);
            Ok(())
        })?;
        Ok(rows)
    }

    #[test]
    fn streams_rows_in_order_and_skips_bom() {
        let rows = collect(b"\xef\xbb\xbf[[\"a\",\"c\",0,\"\",0],[\"b\",\"c\",1,\"\",0]]").unwrap();
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn error_names_file_and_row() {
        let err = collect(br#"[["a","c",0,"",0],["b","c","x","",0]]"#)
            .unwrap_err()
            .to_string();
        assert!(
            err.starts_with("tag_bank_1.json: row 1: field 2 (order)"),
            "{err}"
        );
    }

    #[test]
    fn rejects_non_array_and_trailing_data() {
        assert!(collect(br#"{"a":1}"#).is_err());
        let err = collect(br#"[] []"#).unwrap_err().to_string();
        assert!(err.contains("data after the top-level array"), "{err}");
    }

    #[test]
    fn handler_error_is_passed_through() {
        let mut reader: &[u8] = br#"[["a","c",0,"",0]]"#;
        let err = stream_rows(&mut reader, "t", |_row: TagRow| {
            Err(FormatError::MissingIndex)
        })
        .unwrap_err();
        assert!(matches!(err, FormatError::MissingIndex));
    }
}
