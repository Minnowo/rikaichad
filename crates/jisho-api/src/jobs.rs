// SPDX-License-Identifier: AGPL-3.0-or-later
//! Background jobs (dictionary imports), and the single write slot that
//! imports and removals take (one store write at a time, docs/api.md).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::Serialize;

use crate::dictionary_json::DictionaryJson;
use crate::timestamp::rfc3339_utc;

/// Finished jobs kept for `GET /v1/jobs/{id}`; older ones are forgotten.
const KEEP_FINISHED: usize = 32;

/// State of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// A job as `GET /v1/jobs/{id}` shows it.
#[derive(Debug, Clone, Serialize)]
pub struct JobJson {
    pub id: u64,
    pub kind: &'static str,
    pub state: JobState,
    /// The uploaded file's name.
    pub source_name: String,
    pub progress: ProgressJson,
    pub created_at: String,
    pub finished_at: Option<String>,
    /// The imported dictionary, once succeeded.
    pub dictionary: Option<DictionaryJson>,
    /// Non-fatal problems found while importing.
    pub warnings: Vec<String>,
    /// Why it failed: the error envelope's `code` and `message`.
    pub error: Option<JobErrorJson>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProgressJson {
    pub files_done: usize,
    pub files_total: usize,
    pub rows: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobErrorJson {
    pub code: &'static str,
    pub message: String,
}

struct Job {
    json: JobJson,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    jobs: Vec<Job>,
    /// An import or removal holds the store's write slot.
    writing: bool,
}

/// All jobs of the server.
#[derive(Default)]
pub struct Jobs {
    inner: Mutex<Inner>,
}

/// The write slot, released when dropped.
pub struct WriteSlot {
    jobs: Arc<Jobs>,
}

impl Drop for WriteSlot {
    fn drop(&mut self) {
        self.jobs.lock().writing = false;
    }
}

impl Jobs {
    /// Takes the write slot, or `None` while another write runs.
    pub fn try_write(self: &Arc<Self>) -> Option<WriteSlot> {
        let mut inner = self.lock();
        if inner.writing {
            return None;
        }
        inner.writing = true;
        Some(WriteSlot {
            jobs: Arc::clone(self),
        })
    }

    /// Registers a running import job; returns its id and cancel flag.
    pub fn start_import(&self, source_name: &str, now: i64) -> (u64, Arc<AtomicBool>) {
        let mut inner = self.lock();
        inner.next_id += 1;
        let id = inner.next_id;
        let cancel = Arc::new(AtomicBool::new(false));
        inner.jobs.push(Job {
            json: JobJson {
                id,
                kind: "import",
                state: JobState::Running,
                source_name: source_name.to_string(),
                progress: ProgressJson::default(),
                created_at: rfc3339_utc(now),
                finished_at: None,
                dictionary: None,
                warnings: Vec::new(),
                error: None,
            },
            cancel: Arc::clone(&cancel),
        });
        (id, cancel)
    }

    pub fn update(&self, id: u64, change: impl FnOnce(&mut JobJson)) {
        let mut inner = self.lock();
        if let Some(job) = inner.jobs.iter_mut().find(|j| j.json.id == id) {
            change(&mut job.json);
        }
    }

    /// Marks job `id` finished with `state` at `now`, then forgets the
    /// oldest finished jobs beyond [`KEEP_FINISHED`].
    pub fn finish(&self, id: u64, now: i64, change: impl FnOnce(&mut JobJson)) {
        let mut inner = self.lock();
        if let Some(job) = inner.jobs.iter_mut().find(|j| j.json.id == id) {
            change(&mut job.json);
            job.json.finished_at = Some(rfc3339_utc(now));
        }
        let finished = inner
            .jobs
            .iter()
            .filter(|j| j.json.state != JobState::Running)
            .count();
        let mut excess = finished.saturating_sub(KEEP_FINISHED);
        inner.jobs.retain(|j| {
            if excess > 0 && j.json.state != JobState::Running {
                excess -= 1;
                return false;
            }
            true
        });
    }

    pub fn get(&self, id: u64) -> Option<JobJson> {
        self.lock()
            .jobs
            .iter()
            .find(|j| j.json.id == id)
            .map(|j| j.json.clone())
    }

    /// Cancels a running job (it finishes as cancelled shortly after) or
    /// forgets a finished one. Returns the job as it was, if it exists.
    pub fn cancel_or_forget(&self, id: u64) -> Option<JobJson> {
        let mut inner = self.lock();
        let position = inner.jobs.iter().position(|j| j.json.id == id)?;
        let job = inner.jobs.get(position)?;
        let json = job.json.clone();
        if json.state == JobState::Running {
            job.cancel.store(true, Ordering::Relaxed);
        } else {
            inner.jobs.remove(position);
        }
        Some(json)
    }

    /// Asks every running job to stop (server shutdown).
    pub fn cancel_all(&self) {
        for job in &self.lock().jobs {
            if job.json.state == JobState::Running {
                job.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Marks a job failed if its task ends without finishing it (a panic), so
/// it never stays `running`.
pub struct FinishGuard<'a> {
    pub jobs: &'a Jobs,
    pub id: u64,
    pub now: fn() -> i64,
}

impl Drop for FinishGuard<'_> {
    fn drop(&mut self) {
        if self.jobs.get(self.id).map(|j| j.state) == Some(JobState::Running) {
            self.jobs.finish(self.id, (self.now)(), |job| {
                job.state = JobState::Failed;
                job.error = Some(JobErrorJson {
                    code: "internal",
                    message: "the import stopped unexpectedly".to_string(),
                });
            });
        }
    }
}
