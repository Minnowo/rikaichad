// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GET /v1/dictionaries`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

use crate::dictionary_json::{DictionaryJson, DictionaryListResponse, LookupSettingsJson};
use crate::error::ApiError;
use crate::response::json_response;
use crate::state::AppState;

pub async fn dictionaries_list(State(state): State<AppState>) -> Result<Response, ApiError> {
    let (infos, settings) = state
        .store_work(|store| Ok((store.dictionaries()?, store.lookup_settings()?)))
        .await?;
    let response = DictionaryListResponse {
        dictionaries: infos.into_iter().map(DictionaryJson::from_info).collect(),
        lookup_settings: LookupSettingsJson::from_settings(settings),
    };
    Ok(json_response(StatusCode::OK, &response))
}
