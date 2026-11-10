// SPDX-License-Identifier: AGPL-3.0-or-later
//! `POST /v1/terms/lookup`.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use crate::error::ApiError;
use crate::json_body::read_json;
use crate::lookup_json::{
    DEFAULT_LIMIT, DEFAULT_SCAN_LENGTH, LookupMode, LookupRequest as Lookup, MAX_SCAN_LENGTH,
    run_lookup,
};
use crate::response::json_response;
use crate::state::AppState;

const MAX_TEXT_CHARS: usize = 1000;
const MAX_LIMIT: usize = 1000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LookupRequest {
    text: String,
    mode: Option<String>,
    limit: Option<usize>,
    primary_reading: Option<String>,
    scan_length: Option<usize>,
}

pub async fn terms_lookup(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let request: LookupRequest = read_json(&headers, body, state.config.max_json_body).await?;
    let chars = request.text.chars().count();
    if chars == 0 || chars > MAX_TEXT_CHARS {
        return Err(ApiError::bad_request(format!(
            "\"text\" must be 1 to {MAX_TEXT_CHARS} characters, got {chars}"
        )));
    }
    let mode = match request.mode.as_deref() {
        Some("scan") | None => LookupMode::Scan,
        Some("exact") => LookupMode::Exact,
        Some(other) => {
            return Err(ApiError::bad_request(format!(
                "\"mode\" must be \"scan\" or \"exact\", got {other:?}"
            )));
        }
    };
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::bad_request(format!(
            "\"limit\" must be 1 to {MAX_LIMIT}, got {limit}"
        )));
    }
    let scan_length = request.scan_length.unwrap_or(DEFAULT_SCAN_LENGTH);
    if !(1..=MAX_SCAN_LENGTH).contains(&scan_length) {
        return Err(ApiError::bad_request(format!(
            "\"scan_length\" must be 1 to {MAX_SCAN_LENGTH}, got {scan_length}"
        )));
    }
    let lookup = Lookup {
        text: request.text,
        mode,
        limit,
        primary_reading: request.primary_reading.unwrap_or_default(),
        scan_length,
        max_bytes: state.config.max_response_bytes,
    };
    let response = state
        .store_work(move |store| Ok(run_lookup(store, &lookup)?))
        .await?;
    Ok(json_response(StatusCode::OK, &response))
}
