// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parsing and validation of Yomitan dictionary zips.
//!
//! A dictionary is a zip with `index.json`, numbered bank files
//! (`term_bank_N.json`, `term_meta_bank_N.json`, `kanji_bank_N.json`,
//! `kanji_meta_bank_N.json`, `tag_bank_N.json`) and an optional
//! `styles.css`, all at the archive root. See
//! `docs/reference/yomitan-schemas/` for the format and
//! `docs/dictionary-survey.md` for quirks seen in real dictionaries.
//!
//! Everything read here is untrusted: sizes are capped by [`Limits`],
//! entries are never extracted to disk, and only root-level exact names are
//! treated as dictionary files. Media files are ignored (text only).

mod archive;
mod entry_name;
mod error;
mod glossary;
mod index;
mod limits;
mod render_text;
mod rows;
mod stream;

pub use archive::{Archive, EntryInfo, Warning};
pub use entry_name::EntryKind;
pub use error::FormatError;
pub use glossary::normalize_glossary;
pub use index::{FrequencyMode, Index, IndexTag};
pub use limits::Limits;
pub use render_text::glossary_item_text;
pub use rows::{KanjiMetaRow, KanjiRow, TagRow, TermMetaRow, TermRow};
pub use stream::stream_rows;
