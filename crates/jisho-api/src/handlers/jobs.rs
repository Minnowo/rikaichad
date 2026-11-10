// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GET /v1/jobs/{id}`, `DELETE /v1/jobs/{id}`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Serialize;

use crate::error::ApiError;
use crate::jobs::JobJson;
use crate::response::json_response;
use crate::state::AppState;

#[derive(Serialize)]
struct JobResponse {
    job: JobJson,
}

pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id = job_id(&id)?;
    let job = state.jobs.get(id).ok_or_else(|| not_found(id))?;
    Ok(json_response(StatusCode::OK, &JobResponse { job }))
}

/// Cancels a running job (`202`; it ends as `cancelled` shortly) or
/// forgets a finished one (`200`).
pub async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    state.require_writable()?;
    let id = job_id(&id)?;
    let job = state
        .jobs
        .cancel_or_forget(id)
        .ok_or_else(|| not_found(id))?;
    let status = if job.state == crate::jobs::JobState::Running {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok(json_response(status, &JobResponse { job }))
}

fn not_found(id: u64) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("no job with id {id}"),
    )
}

fn job_id(text: &str) -> Result<u64, ApiError> {
    text.parse()
        .map_err(|_| ApiError::bad_request(format!("job id must be a number, got {text:?}")))
}
