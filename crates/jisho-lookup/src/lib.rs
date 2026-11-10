// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term lookup ported from Yomitan's translator (see NOTICE): from a text,
//! find the dictionary entries for its longest matching prefixes, undoing
//! inflections and normalizing script and width the way Yomitan does, and
//! sort them with Yomitan's criteria.
//!
//! [`find_terms`] returns entries without glossaries (cheap to rank);
//! [`load_glossaries`] fills in glossaries for the entries a caller keeps.

mod deinflection;
mod entry;
mod find_terms;
mod options;
mod sort;
mod text_variants;
mod utf16;

pub use entry::{
    Definition, FindTermsResult, Headword, InflectionChain, InflectionSource, RuleName, Source,
    TermEntry,
};
pub use find_terms::{find_terms, load_glossaries};
pub use options::{DictionaryOptions, EnabledDictionaries, FindTermsOptions};
pub use utf16::utf16_len;
