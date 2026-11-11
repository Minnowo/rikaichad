// SPDX-License-Identifier: AGPL-3.0-or-later
//! Test helpers shared by the rikaichad crates. Only used as a dev-dependency.
//!
//! [`ZipBuilder`] writes zip archives byte by byte so tests can produce
//! broken or hostile archives (wrong CRC, lying sizes, encryption flags)
//! that normal zip writers refuse to create. [`fixture`] builds small
//! dictionaries from in-memory JSON.

// Test-only crate: a failed setup step should abort the test loudly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

pub mod fixture;
pub mod http;
mod temp_dir;
mod zip_builder;

pub use temp_dir::TempDir;
pub use zip_builder::{Method, ZipBuilder, ZipEntry};
