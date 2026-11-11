// SPDX-License-Identifier: AGPL-3.0-or-later
//! `POST /v1/dictionaries` (zip upload, imported by a job),
//! `DELETE /v1/dictionaries/{id}`, `PATCH /v1/dictionaries/{id}`,
//! `PUT /v1/dictionaries/order`.

use std::io::Write;
use std::path::{Path as FsPath, PathBuf};
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{Body, HttpBody};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::Response;
use rikaichad_format::Limits;
use rikaichad_store::{DictionarySettings, Store, StoreError};
use serde::{Deserialize, Deserializer, Serialize};

use crate::dictionary_json::{DictionaryJson, DictionaryListResponse, LookupSettingsJson};
use crate::error::ApiError;
use crate::jobs::{FinishGuard, JobErrorJson, JobJson, JobState, WriteSlot};
use crate::json_body::read_json;
use crate::response::json_response;
use crate::state::AppState;

/// Longest accepted `filename` query value, in bytes.
const MAX_FILENAME_BYTES: usize = 200;

#[derive(Serialize)]
struct JobResponse {
    job: JobJson,
}

/// `POST /v1/dictionaries?filename=NAME.zip`: stores the uploaded zip next
/// to the store, then imports it in the background; `202` with the job.
pub async fn upload(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    state.require_writable()?;
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(|v| v.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if mime != "application/zip" {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "dictionary uploads must be application/zip",
        ));
    }
    let max = state.config.max_upload_bytes;
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|n| n > max) {
        return Err(too_large(max));
    }
    let filename = upload_filename(uri.query())?;
    let slot = state.jobs.try_write().ok_or_else(busy)?;

    // Deleted when dropped: after the import, on an error, or when the
    // client goes away mid-upload (this future is then dropped).
    let directory = UploadDirectory::create(&state.config.store_path)?;
    let path = directory.path.join(&filename);
    receive(body, &path, max, state.config.idle_timeout).await?;

    let (id, cancel) = state.jobs.start_import(&filename, unix_now());
    let jobs = std::sync::Arc::clone(&state.jobs);
    let store_path = state.config.store_path.clone();
    tokio::task::spawn_blocking(move || {
        run_import(&jobs, id, &store_path, &path, &cancel, slot);
        drop(directory);
    });
    let job = state
        .jobs
        .get(id)
        .ok_or_else(|| ApiError::internal("job vanished"))?;
    let mut response = json_response(StatusCode::ACCEPTED, &JobResponse { job });
    if let Ok(location) = HeaderValue::from_str(&format!("/v1/jobs/{id}")) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    Ok(response)
}

/// Below this average rate an upload that has run for
/// [`MIN_RATE_AFTER`] is dropped, so one slow client cannot hold the write
/// slot for days.
const MIN_RATE_BYTES_PER_SEC: u64 = 64 * 1024;
const MIN_RATE_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

/// Streams `body` into `path`, failing past `max` bytes, when no data
/// arrives for a little less than `idle` (the connection's own idle limit,
/// which would otherwise cut it first with a less clear error), or when it
/// is slower than [`MIN_RATE_BYTES_PER_SEC`] on average.
async fn receive(
    mut body: Body,
    path: &FsPath,
    max: u64,
    idle: std::time::Duration,
) -> Result<(), ApiError> {
    let stalled = || {
        ApiError::new(
            StatusCode::REQUEST_TIMEOUT,
            "timeout",
            "upload stalled or too slow",
        )
    };
    let wait = idle.saturating_sub(idle / 10);
    let started = std::time::Instant::now();
    let mut file = std::fs::File::create(path)
        .map_err(|e| ApiError::internal(format!("cannot store the upload: {e}")))?;
    let mut written: u64 = 0;
    loop {
        let frame = tokio::time::timeout(
            wait,
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)),
        )
        .await
        .map_err(|_| stalled())?;
        let Some(frame) = frame else {
            break;
        };
        let frame =
            frame.map_err(|e| ApiError::bad_request(format!("cannot read the upload: {e}")))?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        written += data.len() as u64;
        if written > max {
            return Err(too_large(max));
        }
        // A disk write can block; keep the async workers free meanwhile.
        tokio::task::block_in_place(|| file.write_all(&data))
            .map_err(|e| ApiError::internal(format!("cannot store the upload: {e}")))?;
        let elapsed = started.elapsed();
        if elapsed > MIN_RATE_AFTER && written < MIN_RATE_BYTES_PER_SEC * elapsed.as_secs() {
            return Err(stalled());
        }
    }
    Ok(())
}

fn run_import(
    jobs: &crate::jobs::Jobs,
    id: u64,
    store_path: &FsPath,
    zip: &FsPath,
    cancel: &std::sync::atomic::AtomicBool,
    slot: WriteSlot,
) {
    let _guard = FinishGuard {
        jobs,
        id,
        now: unix_now,
    };
    let result = Store::open(store_path).and_then(|mut store| {
        let report = store.import_cancellable(
            zip,
            &Limits::default(),
            &mut |progress| {
                jobs.update(id, |job| {
                    job.progress.files_done = progress.files_done;
                    job.progress.files_total = progress.files_total;
                    job.progress.rows = progress.rows;
                });
            },
            cancel,
        )?;
        let info = store
            .dictionaries()?
            .into_iter()
            .find(|d| d.id == report.id)
            .ok_or_else(|| StoreError::NotFound {
                what: format!("with id {}", report.id),
            })?;
        Ok((report, info))
    });
    drop(slot);
    jobs.finish(id, unix_now(), |job| match result {
        Ok((report, info)) => {
            job.state = JobState::Succeeded;
            job.warnings = report
                .warnings
                .iter()
                .map(|w| format!("{}: {}", w.entry, w.message))
                .collect();
            job.dictionary = Some(DictionaryJson::from_info(info));
        }
        Err(StoreError::Cancelled) => job.state = JobState::Cancelled,
        Err(error) => {
            let api = ApiError::from(error);
            job.state = JobState::Failed;
            job.error = Some(JobErrorJson {
                code: api.code,
                message: api.message,
            });
        }
    });
}

/// `DELETE /v1/dictionaries/{id}`.
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    state.require_writable()?;
    let id = dictionary_id(&id)?;
    let slot = state.jobs.try_write().ok_or_else(busy)?;
    let pool = state.pool.clone();
    let removed = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        pool.with_store(|store| -> Result<DictionaryJson, ApiError> {
            let info = store
                .dictionaries()?
                .into_iter()
                .find(|d| d.id == id)
                .ok_or_else(|| not_found(id))?;
            store.remove(id)?;
            Ok(DictionaryJson::from_info(info))
        })
    })
    .await
    .map_err(|e| ApiError::internal(format!("request handler failed: {e}")))??;
    #[derive(Serialize)]
    struct Removed {
        removed: DictionaryJson,
    }
    Ok(json_response(StatusCode::OK, &Removed { removed }))
}

/// Body of `PATCH /v1/dictionaries/{id}`: only the given fields change;
/// `"alias": null` goes back to the title.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DictionaryPatch {
    enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    alias: Option<Option<String>>,
    allow_secondary_searches: Option<bool>,
    parts_of_speech_filter: Option<bool>,
    use_deinflections: Option<bool>,
}

/// `PATCH /v1/dictionaries/{id}`.
pub async fn patch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    state.require_writable()?;
    let id = dictionary_id(&id)?;
    let patch: DictionaryPatch = read_json(&headers, body, state.config.max_json_body).await?;
    let slot = state.jobs.try_write().ok_or_else(busy)?;
    if let Some(Some(alias)) = &patch.alias
        && alias.chars().count() > 200
    {
        return Err(ApiError::bad_request(
            "\"alias\" must be at most 200 characters",
        ));
    }
    let info = state
        .store_work(move |store| {
            let _slot = slot;
            let current = store
                .dictionaries()?
                .into_iter()
                .find(|d| d.id == id)
                .ok_or_else(|| not_found(id))?;
            let mut settings: DictionarySettings = current.settings.clone();
            if let Some(alias) = patch.alias {
                settings.alias = alias.filter(|a| !a.is_empty());
            }
            if let Some(value) = patch.allow_secondary_searches {
                settings.allow_secondary_searches = value;
            }
            if let Some(value) = patch.parts_of_speech_filter {
                settings.parts_of_speech_filter = value;
            }
            if let Some(value) = patch.use_deinflections {
                settings.use_deinflections = value;
            }
            store.update_dictionary(id, &settings, patch.enabled)?;
            store
                .dictionaries()?
                .into_iter()
                .find(|d| d.id == id)
                .ok_or_else(|| not_found(id))
        })
        .await?;
    Ok(json_response(
        StatusCode::OK,
        &DictionaryJson::from_info(info),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderRequest {
    ids: Vec<i64>,
}

/// `PUT /v1/dictionaries/order`: the given ids first, in that order.
pub async fn order(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    state.require_writable()?;
    let request: OrderRequest = read_json(&headers, body, state.config.max_json_body).await?;
    let slot = state.jobs.try_write().ok_or_else(busy)?;
    let response = state
        .store_work(move |store| {
            let _slot = slot;
            store.set_order(&request.ids)?;
            Ok(DictionaryListResponse {
                dictionaries: store
                    .dictionaries()?
                    .into_iter()
                    .map(DictionaryJson::from_info)
                    .collect(),
                lookup_settings: LookupSettingsJson::from_settings(store.lookup_settings()?),
            })
        })
        .await?;
    Ok(json_response(StatusCode::OK, &response))
}

/// Distinguishes a field given as `null` (`Some(None)`) from a missing one
/// (`None`, via `#[serde(default)]`).
pub(crate) fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

/// The upload's file name from `?filename=`, reduced to a safe base name
/// ending in `.zip` (default `upload.zip`).
fn upload_filename(query: Option<&str>) -> Result<String, ApiError> {
    let raw = query
        .into_iter()
        .flat_map(|q| q.split('&'))
        .find_map(|pair| pair.strip_prefix("filename="))
        .map(percent_decode)
        .transpose()?
        .unwrap_or_default();
    let base = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.len() > MAX_FILENAME_BYTES {
        return Err(ApiError::bad_request(format!(
            "\"filename\" must be at most {MAX_FILENAME_BYTES} bytes"
        )));
    }
    if cleaned.is_empty() {
        return Ok("upload.zip".to_string());
    }
    if cleaned.to_ascii_lowercase().ends_with(".zip") {
        Ok(cleaned)
    } else {
        Ok(format!("{cleaned}.zip"))
    }
}

/// Decodes `%XX` escapes and `+` (as space) of a query value.
fn percent_decode(value: &str) -> Result<String, ApiError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'%' => {
                let hex = bytes
                    .get(i + 1..i + 3)
                    .and_then(|h| std::str::from_utf8(h).ok())
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or_else(|| ApiError::bad_request("bad percent escape in the query"))?;
                out.push(hex);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ApiError::bad_request("query is not UTF-8"))
}

/// A directory of its own next to the store for each upload, so uploads
/// never meet and the stored file name is the client's. Removed with its
/// contents when dropped.
struct UploadDirectory {
    path: PathBuf,
}

impl UploadDirectory {
    fn create(store_path: &FsPath) -> Result<UploadDirectory, ApiError> {
        let parent = store_path.parent().unwrap_or_else(|| FsPath::new("."));
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = parent.join(format!(".rikaichad-upload-{nanos}"));
        std::fs::create_dir_all(&path)
            .map_err(|e| ApiError::internal(format!("cannot store the upload: {e}")))?;
        Ok(UploadDirectory { path })
    }
}

impl Drop for UploadDirectory {
    fn drop(&mut self) {
        // Best effort: a leftover upload directory only wastes space.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn too_large(max: u64) -> ApiError {
    ApiError::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload_too_large",
        format!("upload is over {max} bytes (--max-upload)"),
    )
}

/// A dictionary id from the path, as the API's JSON error when it is not
/// a number.
pub(crate) fn dictionary_id(text: &str) -> Result<i64, ApiError> {
    text.parse()
        .map_err(|_| ApiError::bad_request(format!("dictionary id must be a number, got {text:?}")))
}

pub(crate) fn busy() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "busy",
        "another import, removal or settings change is running",
    )
}

fn not_found(id: i64) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("no dictionary with id {id}"),
    )
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
