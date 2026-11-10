// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only access to the entries of a dictionary zip.
//!
//! Entries are read through their raw (compressed) bytes and inflated here
//! with flate2, not through the zip crate's decompressing reader, because
//! that reader fails on a CRC-32 mismatch. Real dictionaries ship with wrong
//! CRCs and intact data (docs/dictionary-survey.md), and Yomitan never checks
//! CRCs, so a mismatch becomes a [`Warning`] here. The inflated size must
//! match the declared size exactly; that is what stops zip bombs.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::path::Path;

use flate2::Crc;
use flate2::read::DeflateDecoder;
use zip::{CompressionMethod, ZipArchive};

use crate::entry_name::{self, EntryKind, INDEX_FILE_NAME};
use crate::error::FormatError;
use crate::limits::Limits;

/// An open dictionary archive.
///
/// Created with [`Archive::open`] (a file) or [`Archive::from_reader`]
/// (anything seekable, e.g. an in-memory buffer). Not thread safe; reading
/// an entry needs `&mut self`.
pub struct Archive<R: Read + Seek> {
    zip: ZipArchive<R>,
    entries: Vec<EntryInfo>,
    limits: Limits,
}

/// One entry of the archive's central directory.
#[derive(Debug, Clone)]
pub struct EntryInfo {
    /// Position in the central directory; also the processing order.
    pub index: usize,
    /// Entry name (lossy UTF-8).
    pub name: String,
    /// What the name says the entry is.
    pub kind: EntryKind,
    /// Declared uncompressed size in bytes.
    pub size: u64,
}

/// A problem that does not stop the import, reported to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Archive entry the warning is about.
    pub entry: String,
    /// Human-readable description.
    pub message: String,
}

impl Archive<BufReader<File>> {
    /// Opens the zip file at `path` and reads its central directory.
    ///
    /// Fails if the file is not a zip or has more entries than
    /// `limits.max_entries`.
    pub fn open(path: &Path, limits: Limits) -> Result<Self, FormatError> {
        let file = File::open(path).map_err(|source| FormatError::Io {
            context: format!("cannot open {}", path.display()),
            source,
        })?;
        Archive::from_reader(BufReader::new(file), limits)
    }
}

impl<R: Read + Seek> Archive<R> {
    /// Reads the central directory from `reader`.
    pub fn from_reader(reader: R, limits: Limits) -> Result<Self, FormatError> {
        let zip = ZipArchive::new(reader)?;
        if zip.len() > limits.max_entries {
            return Err(FormatError::Limit {
                what: "number of zip entries".to_string(),
                limit: limits.max_entries as u64,
                actual: zip.len() as u64,
            });
        }
        let mut entries = Vec::with_capacity(zip.len());
        for index in 0..zip.len() {
            let data = zip.by_index_data(index)?;
            let name = String::from_utf8_lossy(data.name_raw()).into_owned();
            let kind = entry_name::classify(&name);
            entries.push(EntryInfo {
                index,
                name,
                kind,
                size: data.size(),
            });
        }
        Ok(Archive {
            zip,
            entries,
            limits,
        })
    }

    /// All entries in central-directory order.
    pub fn entries(&self) -> &[EntryInfo] {
        &self.entries
    }

    /// The caps this archive was opened with.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Finds the root-level `index.json`.
    ///
    /// Returns [`FormatError::NestedIndex`] when it only exists inside a
    /// directory (a common packaging mistake) and
    /// [`FormatError::MissingIndex`] when it does not exist at all.
    pub fn index_entry(&self) -> Result<EntryInfo, FormatError> {
        if let Some(entry) = self.entries.iter().find(|e| e.kind == EntryKind::Index) {
            return Ok(entry.clone());
        }
        for entry in &self.entries {
            if let Some(prefix) = entry.name.strip_suffix(INDEX_FILE_NAME)
                && prefix.ends_with('/')
                && !prefix.starts_with("__MACOSX/")
            {
                return Err(FormatError::NestedIndex {
                    prefix: prefix.to_string(),
                });
            }
        }
        Err(FormatError::MissingIndex)
    }

    /// Reads one entry, passing a reader of its uncompressed bytes to `f`.
    ///
    /// `max_bytes` caps the declared size. After `f` returns, the rest of the
    /// entry is drained so the size and CRC checks always run. Returns `f`'s
    /// result and a warning if the CRC-32 did not match.
    ///
    /// Errors: encrypted entries, compression other than stored/deflate,
    /// declared size over `max_bytes`, data longer or shorter than declared,
    /// I/O errors (all as `E::from(FormatError)`), and any error from `f`,
    /// passed through unchanged.
    pub fn read_entry<T, E: From<FormatError>>(
        &mut self,
        entry: &EntryInfo,
        max_bytes: u64,
        f: impl FnOnce(&mut dyn Read) -> Result<T, E>,
    ) -> Result<(T, Option<Warning>), E> {
        if entry.size > max_bytes {
            return Err(FormatError::Limit {
                what: format!("uncompressed size of {}", entry.name),
                limit: max_bytes,
                actual: entry.size,
            }
            .into());
        }
        let raw = self
            .zip
            .by_index_raw(entry.index)
            .map_err(|e| E::from(FormatError::from(e)))?;
        if raw.encrypted() {
            return Err(entry_error(entry, "encrypted entries are not supported").into());
        }
        let expected_crc = raw.crc32();
        let inflated: Box<dyn Read + '_> = match raw.compression() {
            CompressionMethod::Stored => Box::new(raw),
            CompressionMethod::Deflated => Box::new(DeflateDecoder::new(raw)),
            other => {
                return Err(entry_error(
                    entry,
                    &format!("unsupported compression method {other:?}"),
                )
                .into());
            }
        };
        let mut checked = CheckedReader {
            inner: inflated,
            remaining: entry.size,
            crc: Crc::new(),
        };
        let value = f(&mut checked)?;
        io::copy(&mut checked, &mut io::sink())
            .map_err(|e| E::from(entry_error(entry, &e.to_string())))?;
        let warning = if checked.crc.sum() == expected_crc {
            None
        } else {
            Some(Warning {
                entry: entry.name.clone(),
                message: format!(
                    "CRC-32 mismatch (header {expected_crc:08x}, data {:08x}); data read anyway",
                    checked.crc.sum()
                ),
            })
        };
        Ok((value, warning))
    }

    /// Reads a whole entry into a string, for small text files such as
    /// `index.json` and `styles.css`.
    pub fn read_to_string(
        &mut self,
        entry: &EntryInfo,
        max_bytes: u64,
    ) -> Result<(String, Option<Warning>), FormatError> {
        let name = entry.name.clone();
        self.read_entry(entry, max_bytes, |reader| -> Result<String, FormatError> {
            let mut text = String::new();
            reader
                .read_to_string(&mut text)
                .map_err(|e| FormatError::Entry {
                    name: name.clone(),
                    message: e.to_string(),
                })?;
            Ok(text)
        })
    }
}

/// Reader that yields exactly `remaining` bytes from `inner`, computing their
/// CRC-32, and fails if `inner` holds more or fewer bytes.
struct CheckedReader<R: Read> {
    inner: R,
    remaining: u64,
    crc: Crc,
}

impl<R: Read> Read for CheckedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            // Probe one byte: data past the declared size means the header
            // lies, which is how zip bombs get past size checks.
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "entry is larger than its declared size",
                )),
            };
        }
        let want = usize::try_from(self.remaining)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let Some(window) = buf.get_mut(..want) else {
            return Ok(0);
        };
        let n = self.inner.read(window)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "entry is smaller than its declared size",
            ));
        }
        if let Some(read) = window.get(..n) {
            self.crc.update(read);
        }
        self.remaining -= n as u64;
        Ok(n)
    }
}

fn entry_error(entry: &EntryInfo, message: &str) -> FormatError {
    FormatError::Entry {
        name: entry.name.clone(),
        message: message.to_string(),
    }
}
