// SPDX-License-Identifier: AGPL-3.0-or-later
//! API errors and their JSON envelope (docs/api.md "Errors").

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rikaichad_store::StoreError;
use serde::Serialize;

use crate::response::json_response;

/// An error response: HTTP status, stable machine-readable `code`, and a
/// human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn internal(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }
}

#[derive(Serialize)]
struct Envelope<'a> {
    error: Body<'a>,
}

#[derive(Serialize)]
struct Body<'a> {
    code: &'a str,
    message: &'a str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let envelope = Envelope {
            error: Body {
                code: self.code,
                message: &self.message,
            },
        };
        json_response(self.status, &envelope)
    }
}

impl From<StoreError> for ApiError {
    fn from(err: StoreError) -> Self {
        let message = err.to_string();
        match err {
            StoreError::Conflict { .. } => ApiError::new(StatusCode::CONFLICT, "conflict", message),
            StoreError::Busy => ApiError::new(StatusCode::CONFLICT, "busy", message),
            StoreError::NotFound { .. } => {
                ApiError::new(StatusCode::NOT_FOUND, "not_found", message)
            }
            StoreError::ReadOnly => ApiError::new(StatusCode::FORBIDDEN, "read_only", message),
            StoreError::Invalid { .. } => ApiError::bad_request(message),
            StoreError::Cancelled => ApiError::new(StatusCode::CONFLICT, "cancelled", message),
            StoreError::Interrupted => crate::state::timeout_error(),
            StoreError::Format(_) | StoreError::InvalidRow { .. } => ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_dictionary",
                message,
            ),
            StoreError::Sqlite(_)
            | StoreError::Corrupt { .. }
            | StoreError::Io { .. }
            | StoreError::UnsupportedStoreVersion { .. } => ApiError::internal(message),
        }
    }
}
