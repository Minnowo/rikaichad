// SPDX-License-Identifier: AGPL-3.0-or-later
//! Responses for unknown paths and wrong methods, in the error envelope.

use axum::http::{StatusCode, Uri};

use crate::error::ApiError;

pub async fn not_found(uri: Uri) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("no endpoint at {}", uri.path()),
    )
}

pub async fn method_not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "method not allowed for this path",
    )
}
