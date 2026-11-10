// SPDX-License-Identifier: AGPL-3.0-or-later
//! HTTP/JSON API for jisho, specified in `docs/api.md`.
//!
//! [`run`] serves the API until Ctrl-C. [`router`] builds the same routes
//! for tests. [`run_lookup`] and the JSON types ([`LookupResponse`],
//! [`DictionaryJson`]) are public so the CLI's `lookup` behaves exactly
//! like the API and its `--json` output is byte-for-byte the API's.

mod config;
mod dictionary_json;
mod error;
mod guard;
mod handlers;
mod jobs;
mod json_body;
mod listener;
mod lookup_json;
mod response;
mod server;
mod state;
mod store_pool;
mod text_length;
mod timestamp;

pub use config::DEFAULT_BIND;
pub use config::ServeConfig;
pub use dictionary_json::{DictionaryJson, DictionaryListResponse, LookupSettingsJson};
pub use error::ApiError;
pub use lookup_json::{
    DEFAULT_LIMIT, DEFAULT_SCAN_LENGTH, DefinitionJson, FrequencyJson, HeadwordJson,
    InflectionChainJson, KanjiEntryJson, KanjiLookupResponse, LookupMode, LookupRequest,
    LookupResponse, MAX_SCAN_LENGTH, MatchedJson, ParseResponse, ParseTokenJson,
    PronunciationItemJson, PronunciationJson, TagJson, TermEntryJson, run_kanji_lookup, run_lookup,
    run_parse,
};
pub use server::{ServerError, router, run};
pub use text_length::TextLength;
