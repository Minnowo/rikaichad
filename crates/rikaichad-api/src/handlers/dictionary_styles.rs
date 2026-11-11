// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GET /v1/dictionaries/{id}/styles`: a dictionary's styles.css, for
//! clients that render glossaries as HTML (the web UI scopes it to the
//! dictionary's definitions, as Yomitan does).

use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::handlers::dictionaries_manage::dictionary_id;
use crate::state::AppState;

/// Returns the styles as `text/css`; `404 not_found` when the dictionary
/// does not exist or has no styles, `400` for a non-numeric id.
pub async fn dictionary_styles(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id = dictionary_id(&id)?;
    let styles = state
        .store_work(move |store| Ok(store.dictionary_styles(id)?))
        .await?;
    let Some(css) = styles else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("dictionary {id} has no styles"),
        ));
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        css,
    )
        .into_response())
}
