// SPDX-License-Identifier: AGPL-3.0-or-later
//! `POST /v1/terms/forms`: the conjugation table of a verb or
//! i-adjective. Needs no store: the forms are generated from the term,
//! its reading and its word classes (as a lookup returns them).

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use rikaichad_lookup::{MAX_CONJUGATION_INPUT, conjugate, is_string_entirely_kana};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::json_body::read_json;
use crate::lookup_json::{FuriganaJson, furigana_json};
use crate::response::json_response;
use crate::state::AppState;

/// Most word classes accepted in one request, and longest class name.
const MAX_WORD_CLASSES: usize = 32;
const MAX_WORD_CLASS_CHARS: usize = 32;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FormsRequest {
    term: String,
    #[serde(default)]
    reading: String,
    word_classes: Vec<String>,
}

#[derive(Serialize)]
struct FormsResponse {
    /// The class the table was built for; null if none is supported.
    word_class: Option<&'static str>,
    term: String,
    reading: String,
    forms: Vec<FormJson>,
}

#[derive(Serialize)]
struct FormJson {
    name: &'static str,
    polite: bool,
    negative: bool,
    term: String,
    reading: String,
    furigana: Vec<FuriganaJson>,
}

/// Handles `POST /v1/terms/forms` (docs/api.md): validates the request
/// and returns the conjugation table, or `word_class: null` and no forms
/// if no class is supported. Read-only; never touches the store.
pub async fn terms_forms(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let request: FormsRequest = read_json(&headers, body, state.config.max_json_body).await?;
    for (name, value) in [("term", &request.term), ("reading", &request.reading)] {
        if value.chars().count() > MAX_CONJUGATION_INPUT {
            return Err(ApiError::bad_request(format!(
                "\"{name}\" must be at most {MAX_CONJUGATION_INPUT} characters"
            )));
        }
    }
    if request.term.is_empty() {
        return Err(ApiError::bad_request("\"term\" must not be empty"));
    }
    if request.reading.is_empty() && !is_string_entirely_kana(&request.term) {
        return Err(ApiError::bad_request(
            "\"reading\" is required when \"term\" is not all kana",
        ));
    }
    if request.word_classes.len() > MAX_WORD_CLASSES
        || request
            .word_classes
            .iter()
            .any(|c| c.chars().count() > MAX_WORD_CLASS_CHARS)
    {
        return Err(ApiError::bad_request(format!(
            "\"word_classes\" must have at most {MAX_WORD_CLASSES} entries of at most \
             {MAX_WORD_CLASS_CHARS} characters"
        )));
    }
    let response = match conjugate(&request.term, &request.reading, &request.word_classes) {
        Some(table) => FormsResponse {
            word_class: Some(table.word_class),
            forms: table
                .forms
                .into_iter()
                .map(|f| FormJson {
                    furigana: furigana_json(&f.term, &f.reading),
                    name: f.name,
                    polite: f.polite,
                    negative: f.negative,
                    term: f.term,
                    reading: f.reading,
                })
                .collect(),
            term: table.term,
            reading: table.reading,
        },
        None => FormsResponse {
            word_class: None,
            reading: if request.reading.is_empty() {
                request.term.clone()
            } else {
                request.reading
            },
            term: request.term,
            forms: Vec::new(),
        },
    };
    Ok(json_response(StatusCode::OK, &response))
}
