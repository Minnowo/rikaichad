// SPDX-License-Identifier: AGPL-3.0-or-later
//! `POST /v1/text/parse`.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use crate::error::ApiError;
use crate::json_body::read_json;
use crate::lookup_json::{DEFAULT_SCAN_LENGTH, MAX_SCAN_LENGTH, run_parse};
use crate::response::json_response;
use crate::state::AppState;

/// A parse runs one lookup per token; this bounds the work per request.
pub const MAX_PARSE_CHARS: usize = 1000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParseRequest {
    text: String,
    scan_length: Option<usize>,
}

pub async fn text_parse(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let request: ParseRequest = read_json(&headers, body, state.config.max_json_body).await?;
    let chars = request.text.chars().count();
    if chars == 0 || chars > MAX_PARSE_CHARS {
        return Err(ApiError::bad_request(format!(
            "\"text\" must be 1 to {MAX_PARSE_CHARS} characters, got {chars}"
        )));
    }
    let scan_length = request.scan_length.unwrap_or(DEFAULT_SCAN_LENGTH);
    if !(1..=MAX_SCAN_LENGTH).contains(&scan_length) {
        return Err(ApiError::bad_request(format!(
            "\"scan_length\" must be 1 to {MAX_SCAN_LENGTH}, got {scan_length}"
        )));
    }
    let text = request.text;
    let response = state
        .store_work(move |store| Ok(run_parse(store, &text, scan_length)?))
        .await?;
    Ok(json_response(StatusCode::OK, &response))
}
