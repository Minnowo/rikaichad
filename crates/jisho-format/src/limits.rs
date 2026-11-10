// SPDX-License-Identifier: AGPL-3.0-or-later
//! Caps applied while reading untrusted dictionary archives.

/// Size and count caps for one dictionary archive.
///
/// Defaults are set well above the largest real dictionaries measured in
/// `docs/dictionary-survey.md` (24k zip entries, 125 MB bank file, 3.5 GB
/// uncompressed) while stopping zip bombs and absurd inputs.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Maximum number of entries in the zip central directory.
    pub max_entries: usize,
    /// Maximum uncompressed size of `index.json`, in bytes.
    pub max_index_bytes: u64,
    /// Maximum uncompressed size of `styles.css`, in bytes.
    pub max_styles_bytes: u64,
    /// Maximum uncompressed size of one bank file, in bytes.
    pub max_bank_bytes: u64,
    /// Maximum total uncompressed size of all bank files, in bytes.
    pub max_total_bank_bytes: u64,
    /// Maximum length of a term, reading, kanji character or tag name, in
    /// bytes. These become index keys in the store.
    pub max_key_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_entries: 200_000,
            max_index_bytes: 1 << 20,
            max_styles_bytes: 8 << 20,
            max_bank_bytes: 1 << 30,
            max_total_bank_bytes: 64 << 30,
            max_key_bytes: 4096,
        }
    }
}
