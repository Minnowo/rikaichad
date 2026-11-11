// SPDX-License-Identifier: AGPL-3.0-or-later
//! Error type for reading and validating dictionary archives.

use std::fmt;

/// Why a dictionary archive could not be read or is invalid.
///
/// Messages name the archive entry and, for bank files, the row and field,
/// so a user can find the problem in the source dictionary.
#[derive(Debug)]
pub enum FormatError {
    /// Reading the archive file failed.
    Io {
        context: String,
        source: std::io::Error,
    },
    /// The zip container itself is malformed.
    Zip { message: String },
    /// No `index.json` at the archive root.
    MissingIndex,
    /// `index.json` exists only inside a directory; `prefix` is that directory.
    NestedIndex { prefix: String },
    /// `index.json` is not valid.
    InvalidIndex { message: String },
    /// The dictionary uses a format version other than 3.
    UnsupportedFormat { version: i64 },
    /// A size or count cap was exceeded.
    Limit {
        what: String,
        limit: u64,
        actual: u64,
    },
    /// An archive entry cannot be read (encryption, compression method,
    /// size mismatch).
    Entry { name: String, message: String },
    /// A bank or index file is not valid JSON or does not match the schema.
    Json { name: String, message: String },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Io { context, source } => write!(f, "{context}: {source}"),
            FormatError::Zip { message } => write!(f, "invalid zip archive: {message}"),
            FormatError::MissingIndex => write!(f, "no index.json at the archive root"),
            FormatError::NestedIndex { prefix } => write!(
                f,
                "index.json is nested in {prefix:?}; it must be at the archive root"
            ),
            FormatError::InvalidIndex { message } => write!(f, "index.json: {message}"),
            FormatError::UnsupportedFormat { version } => write!(
                f,
                "dictionary format {version} is not supported (only format 3)"
            ),
            FormatError::Limit {
                what,
                limit,
                actual,
            } => write!(f, "{what} is {actual}, over the limit of {limit}"),
            FormatError::Entry { name, message } => write!(f, "{name}: {message}"),
            FormatError::Json { name, message } => write!(f, "{name}: {message}"),
        }
    }
}

impl std::error::Error for FormatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FormatError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<zip::result::ZipError> for FormatError {
    fn from(err: zip::result::ZipError) -> Self {
        FormatError::Zip {
            message: err.to_string(),
        }
    }
}
