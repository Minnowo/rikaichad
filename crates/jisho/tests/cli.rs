// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests of the jisho binary at its external interface: arguments in,
//! stdout/stderr and exit status out.

// Helpers below are test-only code; clippy's allow-*-in-tests settings cover
// only #[test] functions, not their helpers.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Output};

use jisho_testkit::{TempDir, fixture};

#[test]
fn version_prints_name_and_package_version() {
    let out = jisho(&["--version"]);
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout, format!("jisho {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_lists_options_and_commands() {
    let out = jisho(&["--help"]);
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("jisho - "));
    for word in ["--version", "--store", "import", "lookup", "serve"] {
        assert!(stdout.contains(word), "help lacks {word}");
    }
}

#[test]
fn import_list_lookup_remove_round_trip() {
    let dir = TempDir::new("cli-roundtrip");
    let store = dir.path().join("sub").join("store.sqlite");
    let zip = dir.write("fixture.zip", &fixture::terms_dictionary("Fixture"));

    let out = jisho_in(&store, &["import", path(&zip)]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("imported \"Fixture\""),
        "{}",
        stderr(&out)
    );

    let out = jisho_in(&store, &["list"]);
    assert!(stdout(&out).contains("Fixture"), "{}", stdout(&out));

    let out = jisho_in(&store, &["lookup", "食べる"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.starts_with("食べる [たべる]  (Fixture)"), "{text}");
    assert!(text.contains("1. to eat"), "{text}");
    assert!(text.contains("2. to consume"), "{text}");

    let out = jisho_in(&store, &["remove", "Fixture"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = jisho_in(&store, &["lookup", "食べる"]);
    assert_eq!(out.status.code(), Some(1), "nothing found exits 1");
    assert!(stdout(&out).is_empty());
}

#[test]
fn json_output_matches_api_shapes() {
    let dir = TempDir::new("cli-json");
    let store = dir.path().join("store.sqlite");
    let zip = dir.write("fixture.zip", &fixture::terms_dictionary("Fixture"));
    assert!(jisho_in(&store, &["import", path(&zip)]).status.success());

    let out = jisho_in(&store, &["lookup", "--json", "カタカナ"]);
    let body: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(body["entries"][0]["reading"], "カタカナ");
    assert_eq!(body["entries"][0]["glossary_text"][0], "katakana");
    assert_eq!(body["matched"]["length"]["chars"], 4);

    let out = jisho_in(&store, &["list", "--json"]);
    let body: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(body["dictionaries"][0]["title"], "Fixture");
    assert_eq!(body["dictionaries"][0]["counts"]["kanji"], 1);
}

#[test]
fn failed_import_reports_and_continues_with_other_zips() {
    let dir = TempDir::new("cli-badzip");
    let store = dir.path().join("store.sqlite");
    let bad = dir.write("bad.zip", b"not a zip");
    let good = dir.write("good.zip", &fixture::terms_dictionary("Good"));
    let out = jisho_in(&store, &["import", path(&bad), path(&good)]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("bad.zip: invalid zip archive"), "{err}");
    assert!(err.contains("imported \"Good\""), "{err}");
    assert!(err.contains("1 of 2 dictionaries failed"), "{err}");
}

#[test]
fn duplicate_import_is_an_error() {
    let dir = TempDir::new("cli-dup");
    let store = dir.path().join("store.sqlite");
    let zip = dir.write("fixture.zip", &fixture::terms_dictionary("Dup"));
    assert!(jisho_in(&store, &["import", path(&zip)]).status.success());
    let out = jisho_in(&store, &["import", path(&zip)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("already imported"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn store_can_come_from_environment() {
    let dir = TempDir::new("cli-env");
    let store = dir.path().join("env.sqlite");
    let out = Command::new(env!("CARGO_BIN_EXE_jisho"))
        .args(["list"])
        .env("JISHO_STORE", &store)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(store.exists());
}

#[test]
fn unknown_argument_fails_with_message() {
    let out = jisho(&["--no-such-flag"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("--no-such-flag"), "stderr was: {stderr}");
    assert!(out.stdout.is_empty());
}

fn jisho(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jisho"))
        .args(args)
        .output()
        .unwrap()
}

fn jisho_in(store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jisho"))
        .arg("--store")
        .arg(store)
        .args(args)
        .output()
        .unwrap()
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap()
}
