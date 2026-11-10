// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GET /v1/source`: where to get the source of this server (AGPL-3.0
//! section 13).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use serde::Serialize;

use crate::response::json_response;
use crate::state::AppState;

#[derive(Serialize)]
struct Source<'a> {
    name: &'static str,
    version: &'static str,
    license: &'static str,
    /// Repository URL (`--source-url`, or set at build time).
    repository: Option<&'a str>,
    /// The commit this binary was built from, or "unknown".
    commit: &'static str,
    /// Whether the build had uncommitted changes ("true", "false",
    /// "unknown").
    dirty: &'static str,
}

pub async fn source(State(state): State<AppState>) -> Response {
    json_response(
        StatusCode::OK,
        &Source {
            name: "jisho",
            version: env!("CARGO_PKG_VERSION"),
            license: "AGPL-3.0-or-later AND GPL-3.0-or-later",
            repository: state.config.source_url.as_deref(),
            commit: env!("JISHO_GIT_COMMIT"),
            dirty: env!("JISHO_GIT_DIRTY"),
        },
    )
}
