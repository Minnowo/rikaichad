// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GET /` and `GET /ui/{file}`: the web UI, built into the binary from
//! ui/dist (see build.rs). Every response carries a Content-Security-Policy
//! that allows scripts, styles and fetches from this server only, so
//! dictionary content (styles.css, structured-content styles) cannot load
//! anything from elsewhere.

use axum::extract::Path;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;

/// One embedded file.
pub struct UiAsset {
    /// File name in ui/dist, also its path under `/ui/`.
    pub name: &'static str,
    /// `Content-Type` header value, from the file extension.
    pub content_type: &'static str,
    /// The file's contents.
    pub bytes: &'static [u8],
}

/// The files of ui/dist, sorted by name.
pub static UI_ASSETS: &[UiAsset] = include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));

const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; \
     style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; \
     form-action 'none'; frame-ancestors 'none'";

/// `GET /`: the UI page.
pub async fn index() -> Result<Response, ApiError> {
    asset_response("index.html")
}

/// `GET /ui/{file}`: one UI file by exact name, `404 not_found` otherwise.
pub async fn asset(Path(name): Path<String>) -> Result<Response, ApiError> {
    asset_response(&name)
}

fn asset_response(name: &str) -> Result<Response, ApiError> {
    let Some(asset) = UI_ASSETS.iter().find(|a| a.name == name) else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no UI file {name:?}"),
        ));
    };
    let mut response = (StatusCode::OK, asset.bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(asset.content_type),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    // Revalidate on every load: a new binary may serve different files
    // under the same names.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(response)
}
