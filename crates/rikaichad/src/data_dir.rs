// SPDX-License-Identifier: AGPL-3.0-or-later
//! Default location of the store file, following each platform's
//! convention for per-user application data.

use std::path::{Path, PathBuf};

/// File name of the store inside the data directory.
const STORE_FILE: &str = "rikaichad.sqlite";

/// Environment variable that overrides the store path.
pub const STORE_ENV: &str = "RIKAICHAD_STORE";

/// The store path: `$RIKAICHAD_STORE` if set, else `<data dir>/rikaichad/rikaichad.sqlite`
/// where the data dir is `$XDG_DATA_HOME` or `~/.local/share` (Linux and
/// other Unix), `~/Library/Application Support` (macOS) or `%APPDATA%`
/// (Windows). Returns `None` if no home directory can be found.
pub fn default_store_path() -> Option<PathBuf> {
    if let Some(path) = non_empty_env(STORE_ENV) {
        return Some(PathBuf::from(path));
    }
    Some(data_dir()?.join("rikaichad").join(STORE_FILE))
}

/// A store at the default path used before the project was renamed from
/// jisho (`<data dir>/jisho/jisho.sqlite`, or `$JISHO_STORE`), if one
/// exists while nothing exists at `store`. Nothing is moved automatically.
pub fn legacy_store(store: &Path) -> Option<PathBuf> {
    if store.exists() || non_empty_env(STORE_ENV).is_some() {
        return None;
    }
    let legacy = match non_empty_env("JISHO_STORE") {
        Some(path) => PathBuf::from(path),
        None => data_dir()?.join("jisho").join("jisho.sqlite"),
    };
    legacy.is_file().then_some(legacy)
}

#[cfg(target_os = "windows")]
fn data_dir() -> Option<PathBuf> {
    non_empty_env("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn data_dir() -> Option<PathBuf> {
    Some(
        PathBuf::from(non_empty_env("HOME")?)
            .join("Library")
            .join("Application Support"),
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn data_dir() -> Option<PathBuf> {
    if let Some(xdg) = non_empty_env("XDG_DATA_HOME") {
        return Some(PathBuf::from(xdg));
    }
    Some(
        PathBuf::from(non_empty_env("HOME")?)
            .join(".local")
            .join("share"),
    )
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}
