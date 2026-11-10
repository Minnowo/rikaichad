// SPDX-License-Identifier: AGPL-3.0-or-later
//! Single-file SQLite store for imported dictionaries.
//!
//! [`Store::open`] creates or opens the store file. [`Store::import`]
//! streams a Yomitan dictionary zip into it in one transaction, so a failed
//! import leaves nothing behind and readers never see half a dictionary.
//! Queries read the same file; WAL mode lets them run during an import.

mod dictionaries;
mod error;
mod import;
mod lookup;
mod records;
mod schema;
mod sha256;
mod store;

pub use error::StoreError;
pub use import::{ImportProgress, ImportReport};
pub use records::{Counts, DictionaryInfo, TermRecord};
pub use schema::STORE_VERSION;
pub use sha256::sha256_hex;
pub use store::Store;
