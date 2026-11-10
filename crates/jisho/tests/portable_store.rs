// SPDX-License-Identifier: AGPL-3.0-or-later
//! Store portability across operating systems. CI runs the writer on
//! Linux and the reader on macOS and Windows against the same file (see
//! .github/workflows/ci.yml). Ignored by default: they need
//! `JISHO_PORTABLE_STORE` to name the store file. scripts/check.sh runs
//! both on one machine so they stay in step with the output format.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

use jisho_testkit::{TempDir, fixture};

#[test]
#[ignore = "run by CI with JISHO_PORTABLE_STORE set"]
fn write_portable_store() {
    let store = store_path();
    let dir = TempDir::new("portable-write");
    let zip = dir.write("fixture.zip", &fixture::terms_dictionary("Portable"));
    let out = Command::new(env!("CARGO_BIN_EXE_jisho"))
        .arg("--store")
        .arg(&store)
        .arg("import")
        .arg(&zip)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The import checkpoints the WAL: the single file must be complete.
    let mut wal = store.clone().into_os_string();
    wal.push("-wal");
    let wal_len = std::fs::metadata(PathBuf::from(wal))
        .map(|m| m.len())
        .unwrap_or(0);
    assert_eq!(wal_len, 0, "WAL left with data after import");
}

#[test]
#[ignore = "run by CI with JISHO_PORTABLE_STORE set"]
fn read_portable_store() {
    let store = store_path();
    let out = Command::new(env!("CARGO_BIN_EXE_jisho"))
        .arg("--store")
        .arg(&store)
        .args(["lookup", "--json", "食べる"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let definition = &body["entries"][0]["definitions"][0];
    assert_eq!(definition["dictionary"]["title"], "Portable");
    assert_eq!(definition["glossary_text"][1], "to consume");
}

fn store_path() -> PathBuf {
    PathBuf::from(std::env::var_os("JISHO_PORTABLE_STORE").expect("set JISHO_PORTABLE_STORE"))
}
