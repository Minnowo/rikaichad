// SPDX-License-Identifier: AGPL-3.0-or-later
//! How glossaries are stored: each row's glossary JSON and its plain-text
//! rendering (one string per item, rendered at import) in one payload,
//! compressed with zstd using a dictionary trained on that imported
//! dictionary's own glossaries.
//!
//! Payload: the glossary JSON's length (u32, little endian), the glossary
//! JSON (an array of items), then the text JSON (an array of strings, one
//! per item). Lookups copy both JSON texts into responses without parsing
//! them.
//!
//! Measured on the user's collection (docs/plan/milestone-3.md): about 4x
//! smaller than plain text, decompressing at about 850 MB/s.

use std::collections::HashMap;

use zstd::bulk::{Compressor, Decompressor};
use zstd::dict::DecoderDictionary;

use crate::error::StoreError;

/// zstd level for glossaries.
const LEVEL: i32 = 9;
/// Size of the trained dictionary (zstd's recommended ~100 KB).
const DICTIONARY_BYTES: usize = 112_640;
/// Glossary bytes collected to train a dictionary before compressing.
const TRAINING_BYTES: usize = 4 << 20;

/// A stored glossary, decompressed: both parts are JSON arrays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGlossary {
    /// The glossary items as imported.
    pub json: String,
    /// The plain-text rendering, one string per item.
    pub text: String,
}

impl RawGlossary {
    /// Bytes of both parts, the size a response grows by.
    pub fn len(&self) -> usize {
        self.json.len() + self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub(crate) fn encode(json: &str, text: &str) -> Vec<u8> {
    let length = u32::try_from(json.len()).unwrap_or(u32::MAX);
    let mut payload = Vec::with_capacity(4 + json.len() + text.len());
    payload.extend_from_slice(&length.to_le_bytes());
    payload.extend_from_slice(json.as_bytes());
    payload.extend_from_slice(text.as_bytes());
    payload
}

fn decode(payload: Vec<u8>) -> Result<RawGlossary, String> {
    let (head, rest) = payload
        .split_first_chunk::<4>()
        .ok_or_else(|| "glossary payload too short".to_string())?;
    let length = usize::try_from(u32::from_le_bytes(*head)).unwrap_or(usize::MAX);
    if length > rest.len() {
        return Err("glossary payload length out of range".to_string());
    }
    let (json, text) = rest.split_at(length);
    Ok(RawGlossary {
        json: String::from_utf8(json.to_vec()).map_err(|e| e.to_string())?,
        text: String::from_utf8(text.to_vec()).map_err(|e| e.to_string())?,
    })
}

/// Compresses an imported dictionary's glossaries: collects the first
/// payloads until there is enough to train a dictionary, then compresses
/// them and every later one with it.
pub(crate) struct GlossaryWriter {
    pending: Vec<(i64, Vec<u8>)>,
    pending_bytes: usize,
    compressor: Option<Compressor<'static>>,
    dictionary: Vec<u8>,
}

impl GlossaryWriter {
    pub fn new() -> GlossaryWriter {
        GlossaryWriter {
            pending: Vec::new(),
            pending_bytes: 0,
            compressor: None,
            dictionary: Vec::new(),
        }
    }

    /// Adds the payload of term `id`; returns the rows ready to insert as
    /// (id, uncompressed size, compressed payload).
    pub fn push(
        &mut self,
        id: i64,
        payload: Vec<u8>,
    ) -> Result<Vec<(i64, usize, Vec<u8>)>, StoreError> {
        if let Some(compressor) = &mut self.compressor {
            let size = payload.len();
            let compressed = compressor.compress(&payload).map_err(io)?;
            return Ok(vec![(id, size, compressed)]);
        }
        self.pending_bytes += payload.len();
        self.pending.push((id, payload));
        if self.pending_bytes < TRAINING_BYTES {
            return Ok(Vec::new());
        }
        self.train_and_flush()
    }

    /// Trains on what is pending (if not done yet) and returns the rest.
    pub fn finish(&mut self) -> Result<Vec<(i64, usize, Vec<u8>)>, StoreError> {
        if self.compressor.is_some() {
            return Ok(Vec::new());
        }
        self.train_and_flush()
    }

    /// The trained dictionary (empty when there were too few samples).
    pub fn dictionary(&self) -> &[u8] {
        &self.dictionary
    }

    fn train_and_flush(&mut self) -> Result<Vec<(i64, usize, Vec<u8>)>, StoreError> {
        let samples: Vec<&[u8]> = self.pending.iter().map(|(_, p)| p.as_slice()).collect();
        // Training fails on too few or too uniform samples; compress without
        // a dictionary then.
        self.dictionary = zstd::dict::from_samples(&samples, DICTIONARY_BYTES).unwrap_or_default();
        let mut compressor = Compressor::with_dictionary(LEVEL, &self.dictionary).map_err(io)?;
        let mut rows = Vec::with_capacity(self.pending.len());
        for (id, payload) in std::mem::take(&mut self.pending) {
            let compressed = compressor.compress(&payload).map_err(io)?;
            rows.push((id, payload.len(), compressed));
        }
        self.pending_bytes = 0;
        self.compressor = Some(compressor);
        Ok(rows)
    }
}

/// Prepared decompression dictionaries by dictionary id, kept by a
/// [`crate::Store`] across lookups (preparing one takes longer than
/// decompressing a glossary). Dictionary ids are never reused, so entries
/// never go stale.
#[derive(Default)]
pub(crate) struct DecoderCache {
    dictionaries: HashMap<i64, DecoderDictionary<'static>>,
}

impl DecoderCache {
    pub fn contains(&self, dictionary: i64) -> bool {
        self.dictionaries.contains_key(&dictionary)
    }

    pub fn insert(&mut self, dictionary: i64, trained: &[u8]) {
        self.dictionaries
            .insert(dictionary, DecoderDictionary::copy(trained));
    }

    /// Decompresses and splits rows `(dictionary id, uncompressed size,
    /// payload)`; every dictionary must be in the cache.
    pub fn decode_all(
        &self,
        rows: Vec<(i64, i64, usize, Vec<u8>)>,
    ) -> Result<Vec<(i64, RawGlossary)>, StoreError> {
        let mut decompressor = Decompressor::new().map_err(io)?;
        let mut out = Vec::with_capacity(rows.len());
        for (id, dictionary, size, payload) in rows {
            let prepared =
                self.dictionaries
                    .get(&dictionary)
                    .ok_or_else(|| StoreError::Corrupt {
                        message: format!("no compression dictionary for dictionary {dictionary}"),
                    })?;
            decompressor.set_prepared_dictionary(prepared).map_err(io)?;
            let bytes = decompressor.decompress(&payload, size).map_err(io)?;
            let glossary = decode(bytes).map_err(|message| StoreError::Corrupt {
                message: format!("glossary of term {id}: {message}"),
            })?;
            out.push((id, glossary));
        }
        Ok(out)
    }
}

fn io(error: std::io::Error) -> StoreError {
    StoreError::Io {
        context: "glossary compression".to_string(),
        source: error,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn payload_round_trips_with_and_without_training() {
        for count in [3usize, 20_000] {
            let mut writer = GlossaryWriter::new();
            let mut rows = Vec::new();
            for i in 0..count {
                let json = format!(
                    r#"["meaning number {i}",{{"type":"structured-content","content":"x{}"}}]"#,
                    i % 97
                );
                let text = format!(r#"["meaning number {i}","x{}"]"#, i % 97);
                rows.extend(writer.push(i as i64, encode(&json, &text)).unwrap());
            }
            rows.extend(writer.finish().unwrap());
            assert_eq!(rows.len(), count);
            let mut cache = DecoderCache::default();
            cache.insert(1, writer.dictionary());
            let compressed: usize = rows.iter().map(|r| r.2.len()).sum();
            let raw: usize = rows.iter().map(|r| r.1).sum();
            if count > 3 {
                assert!(!writer.dictionary().is_empty());
                assert!(compressed * 2 < raw, "{compressed} vs {raw}");
            }
            let decoded = cache
                .decode_all(
                    rows.into_iter()
                        .map(|(id, size, p)| (id, 1, size, p))
                        .collect(),
                )
                .unwrap();
            let (id, last) = decoded.last().unwrap();
            assert!(last.json.contains(&format!("meaning number {id}")));
            assert!(last.text.starts_with(&format!(r#"["meaning number {id}""#)));
        }
    }
}
