// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GET /v1/settings`, `PATCH /v1/settings`: store-wide lookup settings.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use jisho_store::FrequencyOrder;
use serde::Deserialize;

use crate::dictionary_json::LookupSettingsJson;
use crate::error::ApiError;
use crate::handlers::dictionaries_manage::present;
use crate::json_body::read_json;
use crate::response::json_response;
use crate::state::AppState;

pub async fn get_settings(State(state): State<AppState>) -> Result<Response, ApiError> {
    let settings = state
        .store_work(|store| Ok(store.lookup_settings()?))
        .await?;
    Ok(json_response(
        StatusCode::OK,
        &LookupSettingsJson::from_settings(settings),
    ))
}

/// Only the given fields change; a dictionary given as `null` clears it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsPatch {
    #[serde(default, deserialize_with = "present")]
    main_dictionary: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    sort_frequency_dictionary: Option<Option<String>>,
    sort_frequency_order: Option<String>,
}

pub async fn patch_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    state.require_writable()?;
    let patch: SettingsPatch = read_json(&headers, body, state.config.max_json_body).await?;
    let order = match patch.sort_frequency_order.as_deref() {
        None => None,
        Some(text) => Some(FrequencyOrder::parse(text).ok_or_else(|| {
            ApiError::bad_request(format!(
                "\"sort_frequency_order\" must be \"ascending\" or \"descending\", got {text:?}"
            ))
        })?),
    };
    let slot = state
        .jobs
        .try_write()
        .ok_or_else(crate::handlers::dictionaries_manage::busy)?;
    let settings = state
        .store_work(move |store| {
            let _slot = slot;
            let titles: Vec<String> = store
                .lookup_dictionaries()?
                .into_iter()
                .map(|d| d.title)
                .collect();
            let check = |field: &str, title: &Option<String>| match title {
                Some(t) if !titles.contains(t) => Err(ApiError::bad_request(format!(
                    "\"{field}\": no dictionary titled {t:?}"
                ))),
                _ => Ok(()),
            };
            let mut settings = store.lookup_settings()?;
            if let Some(main) = patch.main_dictionary {
                check("main_dictionary", &main)?;
                settings.main_dictionary = main;
            }
            if let Some(sort) = patch.sort_frequency_dictionary {
                check("sort_frequency_dictionary", &sort)?;
                settings.sort_frequency_dictionary = sort;
            }
            if let Some(order) = order {
                settings.sort_frequency_order = order;
            }
            store.set_lookup_settings(&settings)?;
            Ok(settings)
        })
        .await?;
    Ok(json_response(
        StatusCode::OK,
        &LookupSettingsJson::from_settings(settings),
    ))
}
