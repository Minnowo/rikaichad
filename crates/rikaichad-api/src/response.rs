// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON response construction.

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::Serialize;

/// Serializes `value` as the body of a response with `status`. A
/// serialization failure (only possible for non-string map keys, which
/// these types do not have) becomes a 500 with a fixed body.
pub fn json_response<T: Serialize>(status: StatusCode, value: &T) -> Response {
    let (status, body) = match serde_json::to_vec(value) {
        Ok(body) => (status, body),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            br#"{"error":{"code":"internal","message":"response serialization failed"}}"#.to_vec(),
        ),
    };
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}
