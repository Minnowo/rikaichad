// SPDX-License-Identifier: AGPL-3.0-or-later
//! `POST /v1/terms/lookup`.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use crate::error::ApiError;
use crate::json_body::read_json;
use crate::lookup_json::lookup_response;
use crate::response::json_response;
use crate::state::AppState;

const MAX_TEXT_CHARS: usize = 1000;
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LookupRequest {
    text: String,
    mode: Option<String>,
    limit: Option<usize>,
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
    match request.mode.as_deref() {
        Some("exact") => {}
        Some("scan") | None => {
            return Err(ApiError::bad_request(
                "\"mode\": \"scan\" is not implemented yet (milestone 2); use \"exact\"",
            ));
        }
        Some(other) => {
            return Err(ApiError::bad_request(format!(
                "\"mode\" must be \"exact\" or \"scan\", got {other:?}"
            )));
        }
    }
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::bad_request(format!(
            "\"limit\" must be 1 to {MAX_LIMIT}, got {limit}"
        )));
    }
    let text = request.text;
    let max_bytes = state.config.max_response_bytes;
    let response = state
        .store_work(move |store| {
            // One extra hit tells whether `limit` cut the results.
            let mut hits = store.find_terms(std::slice::from_ref(&text), limit + 1)?;
            let over_limit = hits.len() > limit;
            hits.truncate(limit);
            let (records, over_budget) = store.with_glossaries_within(hits, max_bytes)?;
            let mut response = lookup_response(&text, records, limit, max_bytes);
            response.truncated |= over_limit || over_budget;
            Ok(response)
        })
        .await?;
    Ok(json_response(StatusCode::OK, &response))
}
