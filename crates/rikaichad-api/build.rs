// SPDX-License-Identifier: AGPL-3.0-or-later
//! Embeds the commit the binary is built from, for `GET /v1/source`
//! (AGPL-3.0 section 13, docs/brief.md): RIKAICHAD_GIT_COMMIT and
//! RIKAICHAD_GIT_DIRTY from git, or from the environment when building outside
//! a checkout (e.g. a source tarball); "unknown" otherwise. RIKAICHAD_SOURCE_URL
//! passes through as the default repository URL.
//!
//! Only a git checkout whose top level is this workspace counts (not a
//! repository the source happens to sit in). Dirty means any change git
//! sees, untracked files included. The script reruns whenever the
//! workspace's sources or the checked-out commit change, so the values
//! cannot go stale.
//!
//! Also writes `ui_assets.rs` into OUT_DIR: the files of ui/dist (the built
//! web UI, committed) as `include_bytes!` entries for `handlers::ui`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    for name in [
        "RIKAICHAD_GIT_COMMIT",
        "RIKAICHAD_GIT_DIRTY",
        "RIKAICHAD_SOURCE_URL",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let root = manifest.join("../..");
    let root = root.canonicalize().unwrap_or(root);
    for watched in [
        "crates",
        "Cargo.toml",
        "Cargo.lock",
        "scripts",
        "docs",
        "ui/src",
        "ui/dist",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(watched).display());
    }
    let toplevel = git(&root, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .and_then(|p| p.canonicalize().ok());
    let in_checkout = toplevel.as_deref() == Some(root.as_path());
    if in_checkout {
        // HEAD may live in a worktree's git dir, branch refs in the common
        // dir.
        for (flag, files) in [
            ("--git-dir", &["HEAD", "index"][..]),
            ("--git-common-dir", &["packed-refs"][..]),
        ] {
            if let Some(dir) = git(&root, &["rev-parse", "--path-format=absolute", flag]) {
                for file in files {
                    println!("cargo:rerun-if-changed={dir}/{file}");
                }
                if flag == "--git-common-dir"
                    && let Some(head) = git(&root, &["symbolic-ref", "-q", "HEAD"])
                {
                    println!("cargo:rerun-if-changed={dir}/{head}");
                }
            }
        }
    }
    let from_git = |args: &[&str]| if in_checkout { git(&root, args) } else { None };
    let commit = std::env::var("RIKAICHAD_GIT_COMMIT")
        .ok()
        .or_else(|| from_git(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());
    let dirty = std::env::var("RIKAICHAD_GIT_DIRTY")
        .ok()
        .or_else(|| from_git(&["status", "--porcelain"]).map(|s| (!s.is_empty()).to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let url = std::env::var("RIKAICHAD_SOURCE_URL").unwrap_or_default();
    println!("cargo:rustc-env=RIKAICHAD_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=RIKAICHAD_GIT_DIRTY={dirty}");
    println!("cargo:rustc-env=RIKAICHAD_SOURCE_URL={url}");
    write_ui_assets(&root.join("ui/dist"));
}

/// Writes the table of UI files. A missing or unreadable ui/dist, or a file
/// type the server does not know, becomes a `compile_error!` naming the
/// problem, so a broken checkout cannot build a binary without its UI.
fn write_ui_assets(dist: &Path) {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("ui_assets.rs");
    let code = match ui_asset_entries(dist) {
        Ok(entries) => format!("&[\n{entries}]"),
        Err(message) => format!("compile_error!({message:?})"),
    };
    if let Err(e) = std::fs::write(&out, code) {
        println!("cargo:warning=cannot write {}: {e}", out.display());
    }
}

fn ui_asset_entries(dist: &Path) -> Result<String, String> {
    let read = std::fs::read_dir(dist).map_err(|e| {
        format!(
            "cannot read {}: {e}; build the UI with `npm run build` in ui/",
            dist.display()
        )
    })?;
    let mut names = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dist.display()))?;
        if entry.path().is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    let mut entries = String::new();
    for name in names {
        let content_type = match name.rsplit_once('.').map(|(_, ext)| ext) {
            Some("html") => "text/html; charset=utf-8",
            Some("js") => "text/javascript; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            _ => return Err(format!("ui/dist/{name}: unknown file type")),
        };
        let path = dist.join(&name);
        entries.push_str(&format!(
            "    UiAsset {{ name: {name:?}, content_type: {content_type:?}, bytes: include_bytes!({:?}) }},\n",
            path.display().to_string()
        ));
    }
    Ok(entries)
}
