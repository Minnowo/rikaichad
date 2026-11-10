// SPDX-License-Identifier: AGPL-3.0-or-later
//! JSON shape of dictionary listings (`GET /v1/dictionaries`, and the
//! CLI's `list --json`).

use jisho_store::DictionaryInfo;
use serde::Serialize;

use crate::timestamp::rfc3339_utc;

/// Response of `GET /v1/dictionaries`.
#[derive(Debug, Clone, Serialize)]
pub struct DictionaryListResponse {
    pub dictionaries: Vec<DictionaryJson>,
}

/// One imported dictionary.
#[derive(Debug, Clone, Serialize)]
pub struct DictionaryJson {
    pub id: i64,
    pub title: String,
    pub revision: String,
    pub format: i64,
    pub sequenced: bool,
    pub enabled: bool,
    pub priority: i64,
    pub author: Option<String>,
    pub url: Option<String>,
    pub description: Option<String>,
    pub attribution: Option<String>,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub frequency_mode: Option<&'static str>,
    pub counts: CountsJson,
    pub has_styles: bool,
    pub imported_at: String,
}

/// Row counts of a dictionary.
#[derive(Debug, Clone, Serialize)]
pub struct CountsJson {
    pub terms: u64,
    pub term_meta: u64,
    pub kanji: u64,
    pub kanji_meta: u64,
    pub tags: u64,
}

impl DictionaryJson {
    /// Converts a store record into its JSON form.
    pub fn from_info(info: DictionaryInfo) -> DictionaryJson {
        let index = info.index;
        DictionaryJson {
            id: info.id,
            title: index.title,
            revision: index.revision,
            format: index.format,
            sequenced: index.sequenced,
            enabled: info.enabled,
            priority: info.priority,
            author: index.author,
            url: index.url,
            description: index.description,
            attribution: index.attribution,
            source_language: index.source_language,
            target_language: index.target_language,
            frequency_mode: index.frequency_mode.map(|m| m.as_str()),
            counts: CountsJson {
                terms: info.counts.terms,
                term_meta: info.counts.term_meta,
                kanji: info.counts.kanji,
                kanji_meta: info.counts.kanji_meta,
                tags: info.counts.tags,
            },
            has_styles: info.has_styles,
            imported_at: rfc3339_utc(info.imported_at),
        }
    }
}
