// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reading JSON request bodies with the API's rules: `application/json`
//! only, size cap, unknown fields rejected (by the target type).

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use serde::de::DeserializeOwned;

use crate::error::ApiError;

/// Reads and parses a JSON body of at most `max_bytes`.
///
/// Errors: `415` for another content type, `413` over the cap, `400` for
/// invalid JSON or a body that does not match `T`.
pub async fn read_json<T: DeserializeOwned>(
    headers: &HeaderMap,
    body: Body,
    max_bytes: usize,
) -> Result<T, ApiError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if mime != "application/json" {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "request body must be application/json",
        ));
    }
    let too_large = || {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            format!("request body is over {max_bytes} bytes"),
        )
    };
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|n| n > max_bytes as u64) {
        return Err(too_large());
    }
    let bytes = axum::body::to_bytes(body, max_bytes).await.map_err(|err| {
        if is_length_limit(&err) {
            too_large()
        } else {
            ApiError::bad_request(format!("cannot read request body: {err}"))
        }
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|e| ApiError::bad_request(format!("invalid request: {e}")))
}

/// Whether a body read failed because of the size cap. The limit error
/// type lives in a crate we do not depend on directly, so the error chain
/// is matched by its message ("length limit exceeded", http-body-util
/// `LengthLimitError`).
fn is_length_limit(err: &axum::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(current) = source {
        if current.to_string().contains("length limit exceeded") {
            return true;
        }
        source = current.source();
    }
    false
}
