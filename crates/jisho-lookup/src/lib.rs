// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term and kanji lookup ported from Yomitan's translator (see NOTICE):
//! from a text, find the dictionary entries for its longest matching
//! prefixes, undoing inflections and normalizing script and width the way
//! Yomitan does; group them by the result mode, add tags, frequencies and
//! pronunciations, and sort them with Yomitan's criteria.
//!
//! [`find_terms`] returns entries without glossaries (cheap to rank);
//! [`load_glossaries`] fills in glossaries for the entries a caller keeps.

mod collation;
mod deinflection;
mod entry;
mod find_terms;
mod grouping;
mod kanji;
mod meta;
mod options;
mod parse;
mod sort;
mod tags;
mod text_variants;
mod utf16;

pub use entry::{
    Definition, FindTermsResult, Headword, InflectionChain, InflectionSource, KanjiEntry,
    KanjiFrequency, KanjiStat, Pronunciation, PronunciationKind, RuleName, Source, Tag, TermEntry,
    TermFrequency, TermPronunciation,
};
pub use find_terms::{find_terms, load_glossaries, load_raw_glossaries};
pub use kanji::find_kanji;
pub use options::{DictionaryOptions, EnabledDictionaries, FindTermsOptions, ResultMode};
pub use parse::{ParsedHeadword, ParsedToken, parse_text};
pub use utf16::utf16_len;
