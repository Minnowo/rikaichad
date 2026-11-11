// SPDX-License-Identifier: AGPL-3.0-or-later
//! UTF-16 lengths: Yomitan measures and compares text lengths in UTF-16
//! code units (JavaScript `length`), and ranking depends on them.

/// Length of `text` in UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}
