// SPDX-License-Identifier: AGPL-3.0-or-later
//! HTTP/JSON API for jisho, specified in `docs/api.md`.
//!
//! [`run`] serves the API until Ctrl-C. [`router`] builds the same routes
//! for tests. The JSON types ([`TermEntryJson`], [`DictionaryJson`],
//! [`LookupResponse`]) are public so the CLI's `--json` output is
//! byte-for-byte the API's.

mod config;
mod dictionary_json;
mod error;
mod guard;
mod handlers;
mod json_body;
mod lookup_json;
mod response;
mod server;
mod state;
mod store_pool;
mod text_length;
mod timestamp;

pub use config::DEFAULT_BIND;
pub use config::ServeConfig;
pub use dictionary_json::{DictionaryJson, DictionaryListResponse};
pub use error::ApiError;
pub use lookup_json::{LookupResponse, MatchedJson, TermEntryJson, lookup_response};
pub use server::{ServerError, router, run};
pub use text_length::TextLength;
