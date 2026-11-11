// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GET /v1/health`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use serde::Serialize;

use crate::response::json_response;
use crate::state::AppState;

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
    read_only: bool,
}

pub async fn health(State(state): State<AppState>) -> Response {
    json_response(
        StatusCode::OK,
        &Health {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            read_only: state.config.read_only,
        },
    )
}
