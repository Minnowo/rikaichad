// SPDX-License-Identifier: AGPL-3.0-or-later
//! A minimal zip writer (no zip64, no data descriptors) following PKWARE
//! APPNOTE 6.3.x sections 4.3.7 (local header), 4.3.12 (central directory)
//! and 4.3.16 (end of central directory).

use std::io::Write;

use flate2::Crc;
use flate2::write::DeflateEncoder;

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0605_4b50;
const VERSION: u16 = 20;
const FLAG_ENCRYPTED: u16 = 1 << 0;
const FLAG_UTF8: u16 = 1 << 11;

/// Compression method of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflate,
    /// Writes the given method id without compressing (for "unsupported
    /// method" tests).
    Raw(u16),
}

/// One entry to write. Overrides let tests make headers lie.
#[derive(Debug, Clone)]
pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub method: Method,
    /// Written instead of the real CRC-32.
    pub crc_override: Option<u32>,
    /// Written instead of the real uncompressed size.
    pub size_override: Option<u32>,
    /// Sets the "encrypted" flag (the data is not actually encrypted).
    pub encrypted_flag: bool,
}

/// Builds a zip archive in memory.
#[derive(Debug, Default)]
pub struct ZipBuilder {
    entries: Vec<ZipEntry>,
}

impl ZipBuilder {
    pub fn new() -> Self {
        ZipBuilder::default()
    }

    /// Adds a deflate-compressed entry and returns it for adjustments.
    pub fn add(&mut self, name: &str, data: &[u8]) -> &mut ZipEntry {
        self.entries.push(ZipEntry {
            name: name.to_string(),
            data: data.to_vec(),
            method: Method::Deflate,
            crc_override: None,
            size_override: None,
            encrypted_flag: false,
        });
        let last = self.entries.len() - 1;
        &mut self.entries[last]
    }

    /// Writes the archive. Panics on sizes over 4 GiB (tests only).
    pub fn finish(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for entry in &self.entries {
            let mut crc = Crc::new();
            crc.update(&entry.data);
            let crc = entry.crc_override.unwrap_or(crc.sum());
            let (method_id, payload) = match entry.method {
                Method::Stored => (0u16, entry.data.clone()),
                Method::Deflate => {
                    let mut encoder =
                        DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                    encoder.write_all(&entry.data).unwrap();
                    (8u16, encoder.finish().unwrap())
                }
                Method::Raw(id) => (id, entry.data.clone()),
            };
            let size = entry
                .size_override
                .unwrap_or(u32::try_from(entry.data.len()).unwrap());
            let compressed = u32::try_from(payload.len()).unwrap();
            let name = entry.name.as_bytes();
            let name_len = u16::try_from(name.len()).unwrap();
            let flags = FLAG_UTF8
                | if entry.encrypted_flag {
                    FLAG_ENCRYPTED
                } else {
                    0
                };
            let offset = u32::try_from(out.len()).unwrap();

            put32(&mut out, LOCAL_HEADER_SIGNATURE);
            put16(&mut out, VERSION);
            put16(&mut out, flags);
            put16(&mut out, method_id);
            put16(&mut out, 0); // mod time
            put16(&mut out, 0x21); // mod date: 1980-01-01
            put32(&mut out, crc);
            put32(&mut out, compressed);
            put32(&mut out, size);
            put16(&mut out, name_len);
            put16(&mut out, 0); // extra length
            out.extend_from_slice(name);
            out.extend_from_slice(&payload);

            put32(&mut central, CENTRAL_HEADER_SIGNATURE);
            put16(&mut central, VERSION); // made by
            put16(&mut central, VERSION); // needed
            put16(&mut central, flags);
            put16(&mut central, method_id);
            put16(&mut central, 0);
            put16(&mut central, 0x21);
            put32(&mut central, crc);
            put32(&mut central, compressed);
            put32(&mut central, size);
            put16(&mut central, name_len);
            put16(&mut central, 0); // extra
            put16(&mut central, 0); // comment
            put16(&mut central, 0); // disk
            put16(&mut central, 0); // internal attributes
            put32(&mut central, 0); // external attributes
            put32(&mut central, offset);
            central.extend_from_slice(name);
        }
        let central_offset = u32::try_from(out.len()).unwrap();
        let central_size = u32::try_from(central.len()).unwrap();
        let count = u16::try_from(self.entries.len()).unwrap();
        out.extend_from_slice(&central);
        put32(&mut out, END_OF_CENTRAL_DIRECTORY_SIGNATURE);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, count);
        put16(&mut out, count);
        put32(&mut out, central_size);
        put32(&mut out, central_offset);
        put16(&mut out, 0);
        out
    }
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
