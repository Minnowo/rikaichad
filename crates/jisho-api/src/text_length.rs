// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text lengths in the two units the API reports (docs/api.md).

use serde::Serialize;

/// Length of a string in UTF-16 code units (JavaScript, Yomitan) and in
/// Unicode scalar values (Rust `char`s, Python).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TextLength {
    pub utf16: usize,
    pub chars: usize,
}

impl TextLength {
    /// Measures `text` in both units.
    pub fn of(text: &str) -> TextLength {
        TextLength {
            utf16: text.encode_utf16().count(),
            chars: text.chars().count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_astral_characters_differently() {
        assert_eq!(TextLength::of("abc"), TextLength { utf16: 3, chars: 3 });
        // U+20B9F is outside the BMP: two UTF-16 units, one char.
        assert_eq!(
            TextLength::of("\u{20b9f}る"),
            TextLength { utf16: 3, chars: 2 }
        );
    }
}
