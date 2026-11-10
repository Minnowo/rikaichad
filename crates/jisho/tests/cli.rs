// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests of the jisho binary at its external interface: arguments in,
//! stdout/stderr and exit status out.

// Helpers below are test-only code; clippy's allow-*-in-tests settings cover
// only #[test] functions, not their helpers.
#![allow(clippy::unwrap_used)]

use std::process::{Command, Output};

#[test]
fn version_prints_name_and_package_version() {
    let out = jisho(&["--version"]);
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout, format!("jisho {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_lists_options() {
    let out = jisho(&["--help"]);
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("jisho - "));
    assert!(stdout.contains("--version"));
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
