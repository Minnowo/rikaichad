// SPDX-License-Identifier: AGPL-3.0-or-later
//! `POST /v1/kanji/lookup`.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use crate::error::ApiError;
use crate::json_body::read_json;
use crate::lookup_json::{DEFAULT_LIMIT, run_kanji_lookup};
use crate::response::json_response;
use crate::state::AppState;

/// Each distinct character is looked up; a kanji lookup is usually one
/// character, so the cap is lower than for terms.
const MAX_TEXT_CHARS: usize = 100;
const MAX_LIMIT: usize = 1000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KanjiRequest {
    text: String,
    limit: Option<usize>,
}

pub async fn kanji_lookup(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let request: KanjiRequest = read_json(&headers, body, state.config.max_json_body).await?;
    let chars = request.text.chars().count();
    if chars == 0 || chars > MAX_TEXT_CHARS {
        return Err(ApiError::bad_request(format!(
            "\"text\" must be 1 to {MAX_TEXT_CHARS} characters, got {chars}"
        )));
    }
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::bad_request(format!(
            "\"limit\" must be 1 to {MAX_LIMIT}, got {limit}"
        )));
    }
    let max_bytes = state.config.max_response_bytes;
    let text = request.text;
    let response = state
        .store_work(move |store| Ok(run_kanji_lookup(store, &text, limit, max_bytes)?))
        .await?;
    Ok(json_response(StatusCode::OK, &response))
}
