// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GET /v1/dictionaries`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

use crate::dictionary_json::{DictionaryJson, DictionaryListResponse};
use crate::error::ApiError;
use crate::response::json_response;
use crate::state::AppState;

pub async fn dictionaries_list(State(state): State<AppState>) -> Result<Response, ApiError> {
    let infos = state.store_work(|store| Ok(store.dictionaries()?)).await?;
    let response = DictionaryListResponse {
        dictionaries: infos.into_iter().map(DictionaryJson::from_info).collect(),
    };
    Ok(json_response(StatusCode::OK, &response))
}
