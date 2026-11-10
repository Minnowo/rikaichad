// SPDX-License-Identifier: AGPL-3.0-or-later
//! Term and kanji lookup as used by `POST /v1/terms/lookup`,
//! `POST /v1/kanji/lookup` and the CLI's `lookup` and `kanji`: running the
//! lookup ([`run_lookup`], [`run_kanji_lookup`]) and its JSON shape.

use jisho_lookup::{
    EnabledDictionaries, FindTermsOptions, KanjiEntry, PronunciationKind, ResultMode, Tag,
    TermEntry, distribute_furigana, find_kanji, find_terms, load_raw_glossaries, parse_text,
};
use jisho_store::{Store, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::text_length::TextLength;

/// Bytes reserved for the response fields around `entries`.
pub const ENVELOPE_ALLOWANCE: usize = 1024;

/// How text is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupMode {
    /// Yomitan-style: the longest prefixes of the text, deinflected and
    /// normalized.
    Scan,
    /// Only entries whose term or reading is exactly the text (Yomitan with
    /// deinflection off).
    Exact,
}

/// What to look up and how much to return.
#[derive(Debug, Clone)]
pub struct LookupRequest {
    pub text: String,
    pub mode: LookupMode,
    /// How entries are grouped (Yomitan's result mode; default "group").
    pub result_mode: ResultMode,
    /// At most this many entries.
    pub limit: usize,
    /// Entries with this reading sort first; empty for none.
    pub primary_reading: String,
    /// Scan mode looks at only this many characters from the start of
    /// `text` (Yomitan's scanning.length; default [`DEFAULT_SCAN_LENGTH`]).
    pub scan_length: usize,
    /// Cap on the serialized size of the returned entries, in bytes.
    pub max_bytes: usize,
}

/// Yomitan's default scanning.length.
pub const DEFAULT_SCAN_LENGTH: usize = 16;
/// Largest accepted scan length: the cost of a scan grows steeply with its
/// length on repetitive input (docs/plan/milestone-2.md, review finding 1).
pub const MAX_SCAN_LENGTH: usize = 32;
/// Yomitan's default general.maxResults.
pub const DEFAULT_LIMIT: usize = 32;

/// Response of a term lookup.
#[derive(Debug, Clone, Serialize)]
pub struct LookupResponse {
    /// Each a serialized [`TermEntryJson`]: entries are serialized once,
    /// to measure them for the response cap, and copied from there.
    pub entries: Vec<Box<RawValue>>,
    pub matched: MatchedJson,
    /// True when entries were dropped by `limit` or the size cap.
    pub truncated: bool,
}

/// The part of the input text that produced the results.
#[derive(Debug, Clone, Serialize)]
pub struct MatchedJson {
    pub text: String,
    pub length: TextLength,
}

/// One result: headwords, how they were reached, their definitions and
/// metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermEntryJson {
    pub headwords: Vec<HeadwordJson>,
    pub definitions: Vec<DefinitionJson>,
    /// Inflections undone, one chain per way the match was found; each
    /// chain starts at the inflection nearest the dictionary form (Yomitan's
    /// order), e.g. potential, negative, past for "tabe-rare-naka-tta".
    pub inflections: Vec<InflectionChainJson>,
    /// Text preprocessors applied (e.g. "convertHiraganaToKatakana"), one
    /// chain per way the match was found.
    pub text_processors: Vec<Vec<String>>,
    pub score: f64,
    pub dictionary_index: usize,
    pub is_primary: bool,
    /// UTF-16 length of the longest input prefix that reached this entry.
    pub max_original_text_length: usize,
    pub frequencies: Vec<FrequencyJson>,
    pub pronunciations: Vec<PronunciationJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadwordJson {
    pub term: String,
    pub reading: String,
    /// The term in pieces with their readings, for ruby (empty reading:
    /// read as written).
    pub furigana: Vec<FuriganaJson>,
    pub sources: Vec<SourceJson>,
    pub tags: Vec<TagJson>,
    /// Part-of-speech ids from the dictionary (e.g. "v1").
    pub word_classes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceJson {
    pub original_text: String,
    pub transformed_text: String,
    pub deinflected_text: String,
    pub match_type: String,
    pub match_source: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InflectionChainJson {
    /// "algorithm", "dictionary" or "both".
    pub source: String,
    pub rules: Vec<RuleJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleJson {
    pub name: String,
    pub description: Option<String>,
}

/// An expanded tag (Yomitan's Tag).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagJson {
    pub name: String,
    pub category: String,
    pub order: f64,
    pub score: f64,
    /// The tag's notes from each dictionary that has some.
    pub notes: Vec<String>,
    /// Titles of the dictionaries the tag came from.
    pub dictionaries: Vec<String>,
    /// A part-of-speech tag repeating the previous definition's.
    pub redundant: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionJson {
    pub dictionary: DictionaryRefJson,
    /// Indexes into the entry's `headwords` this definition applies to.
    pub headword_indices: Vec<usize>,
    /// Store row id.
    pub id: i64,
    pub score: f64,
    pub sequences: Vec<i64>,
    pub is_primary: bool,
    pub tags: Vec<TagJson>,
    /// Glossary items as imported (a JSON array, copied from the store).
    pub glossary: Box<RawValue>,
    /// One plain-text rendering per `glossary` item (a JSON array of
    /// strings, rendered at import).
    pub glossary_text: Box<RawValue>,
}

/// A frequency of one headword.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrequencyJson {
    pub headword_index: usize,
    pub dictionary: DictionaryRefJson,
    /// The value; for a string frequency, the first number in it.
    pub frequency: f64,
    pub display_value: Option<String>,
    /// `display_value` is the raw string `frequency` was parsed from.
    pub display_value_parsed: bool,
    /// The frequency applies to this headword's reading only.
    pub has_reading: bool,
    /// "rank-based" or "occurrence-based", from the dictionary.
    pub frequency_mode: Option<String>,
}

/// Pronunciations of one headword from one dictionary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PronunciationJson {
    pub headword_index: usize,
    pub dictionary: DictionaryRefJson,
    pub items: Vec<PronunciationItemJson>,
}

/// A pitch accent or a phonetic transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PronunciationItemJson {
    PitchAccent {
        /// Downstep position (number) or H/L pattern (string).
        position: Value,
        nasal_positions: Vec<Value>,
        devoice_positions: Vec<Value>,
        tags: Vec<TagJson>,
    },
    PhoneticTranscription {
        ipa: String,
        tags: Vec<TagJson>,
    },
}

/// Which dictionary something came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryRefJson {
    pub id: i64,
    pub title: String,
}

/// Runs a lookup: finds and sorts entries, keeps at most `limit`, loads
/// glossaries within `max_bytes`. Blocking (store access).
pub fn run_lookup(store: &Store, request: &LookupRequest) -> Result<LookupResponse, StoreError> {
    let dictionaries = EnabledDictionaries::from_store(store)?;
    let options = FindTermsOptions {
        mode: request.result_mode,
        deinflect: request.mode == LookupMode::Scan,
        primary_reading: request.primary_reading.clone(),
        // One more than asked, to tell whether there were more.
        limit: Some(request.limit.saturating_add(1)),
        ..FindTermsOptions::default()
    };
    let text = match request.mode {
        LookupMode::Scan => char_prefix(&request.text, request.scan_length.min(MAX_SCAN_LENGTH)),
        LookupMode::Exact => request.text.as_str(),
    };
    let mut result = find_terms(store, text, &dictionaries, &options)?;
    let mut truncated = result.truncated || result.entries.len() > request.limit;
    result.entries.truncate(request.limit);
    let budget = request
        .max_bytes
        .saturating_sub(ENVELOPE_ALLOWANCE + text.len() * 2);
    let filled = load_raw_glossaries(store, &mut result.entries, budget)?;
    if filled < result.entries.len() {
        truncated = true;
        result.entries.truncate(filled);
    }
    // The glossary budget does not see chains, sources or tags; cap the
    // serialized entries as a whole.
    let mut entries = Vec::with_capacity(result.entries.len());
    let mut used = 0usize;
    for entry in result.entries {
        let json = serde_json::value::to_raw_value(&entry_json(entry)?).map_err(internal)?;
        let size = json.get().len();
        if used.saturating_add(size) > budget {
            truncated = true;
            break;
        }
        used += size;
        entries.push(json);
    }
    let matched = utf16_prefix(text, result.original_text_length);
    Ok(LookupResponse {
        entries,
        matched: MatchedJson {
            length: TextLength::of(matched),
            text: matched.to_string(),
        },
        truncated,
    })
}

fn furigana_json(term: &str, reading: &str) -> Vec<FuriganaJson> {
    distribute_furigana(term, reading)
        .into_iter()
        .map(|s| FuriganaJson {
            text: s.text,
            reading: s.reading,
        })
        .collect()
}

fn tags_json(tags: Vec<Tag>) -> Vec<TagJson> {
    tags.into_iter()
        .map(|t| TagJson {
            name: t.name,
            category: t.category,
            order: t.order,
            score: t.score,
            notes: t.content,
            dictionaries: t.dictionaries,
            redundant: t.redundant,
        })
        .collect()
}

fn entry_json(entry: TermEntry) -> Result<TermEntryJson, StoreError> {
    let mut definitions = Vec::with_capacity(entry.definitions.len());
    for d in entry.definitions {
        let glossary = d.glossary.ok_or_else(|| StoreError::Corrupt {
            message: format!("glossary of term {} not loaded", d.id),
        })?;
        definitions.push(DefinitionJson {
            dictionary: DictionaryRefJson {
                id: d.dictionary_id,
                title: d.dictionary,
            },
            headword_indices: d.headword_indices,
            id: d.id,
            score: d.score,
            sequences: d.sequences,
            is_primary: d.is_primary,
            tags: tags_json(d.tags),
            glossary: RawValue::from_string(glossary.json).map_err(internal)?,
            glossary_text: RawValue::from_string(glossary.text).map_err(internal)?,
        });
    }
    Ok(TermEntryJson {
        headwords: entry
            .headwords
            .into_iter()
            .map(|h| HeadwordJson {
                furigana: furigana_json(&h.term, &h.reading),
                term: h.term,
                reading: h.reading,
                sources: h
                    .sources
                    .into_iter()
                    .map(|s| SourceJson {
                        original_text: s.original_text,
                        transformed_text: s.transformed_text,
                        deinflected_text: s.deinflected_text,
                        match_type: s.match_type.to_string(),
                        match_source: s.match_source.to_string(),
                        is_primary: s.is_primary,
                    })
                    .collect(),
                tags: tags_json(h.tags),
                word_classes: h.word_classes,
            })
            .collect(),
        definitions,
        inflections: entry
            .inflection_rule_chain_candidates
            .into_iter()
            .map(|c| InflectionChainJson {
                source: c.source.as_str().to_string(),
                rules: c
                    .inflection_rules
                    .into_iter()
                    .map(|r| RuleJson {
                        name: r.name,
                        description: r.description,
                    })
                    .collect(),
            })
            .collect(),
        text_processors: entry.text_processor_rule_chain_candidates,
        score: entry.score,
        dictionary_index: entry.dictionary_index,
        is_primary: entry.is_primary,
        max_original_text_length: entry.max_original_text_length,
        frequencies: entry
            .frequencies
            .into_iter()
            .map(|f| FrequencyJson {
                headword_index: f.headword_index,
                dictionary: DictionaryRefJson {
                    id: f.dictionary_id,
                    title: f.dictionary,
                },
                frequency: f.frequency,
                display_value: f.display_value,
                display_value_parsed: f.display_value_parsed,
                has_reading: f.has_reading,
                frequency_mode: f.frequency_mode,
            })
            .collect(),
        pronunciations: entry
            .pronunciations
            .into_iter()
            .map(|p| PronunciationJson {
                headword_index: p.headword_index,
                dictionary: DictionaryRefJson {
                    id: p.dictionary_id,
                    title: p.dictionary,
                },
                items: p
                    .pronunciations
                    .into_iter()
                    .map(|item| match item.kind {
                        PronunciationKind::PitchAccent {
                            positions,
                            nasal_positions,
                            devoice_positions,
                        } => PronunciationItemJson::PitchAccent {
                            position: positions,
                            nasal_positions,
                            devoice_positions,
                            tags: tags_json(item.tags),
                        },
                        PronunciationKind::PhoneticTranscription { ipa } => {
                            PronunciationItemJson::PhoneticTranscription {
                                ipa,
                                tags: tags_json(item.tags),
                            }
                        }
                    })
                    .collect(),
            })
            .collect(),
    })
}

/// A store glossary that is not valid JSON.
fn internal(error: serde_json::Error) -> StoreError {
    StoreError::Corrupt {
        message: format!("glossary is not valid JSON: {error}"),
    }
}

/// Response of a kanji lookup.
#[derive(Debug, Clone, Serialize)]
pub struct KanjiLookupResponse {
    pub entries: Vec<KanjiEntryJson>,
    /// True when entries were dropped by `limit` or the size cap.
    pub truncated: bool,
}

/// One kanji from one dictionary.
#[derive(Debug, Clone, Serialize)]
pub struct KanjiEntryJson {
    pub character: String,
    pub dictionary: DictionaryRefJson,
    pub onyomi: Vec<String>,
    pub kunyomi: Vec<String>,
    pub tags: Vec<TagJson>,
    /// Stats grouped by category, in the dictionary's order.
    pub stats: Vec<KanjiStatGroupJson>,
    pub meanings: Vec<String>,
    pub frequencies: Vec<KanjiFrequencyJson>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KanjiStatGroupJson {
    pub category: String,
    pub stats: Vec<KanjiStatJson>,
}

/// A stat (e.g. stroke count), described by the dictionary's tag.
#[derive(Debug, Clone, Serialize)]
pub struct KanjiStatJson {
    pub name: String,
    pub category: String,
    /// The tag's notes (e.g. "Stroke count").
    pub notes: String,
    pub order: f64,
    pub score: f64,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KanjiFrequencyJson {
    pub dictionary: DictionaryRefJson,
    pub frequency: f64,
    pub display_value: Option<String>,
    pub display_value_parsed: bool,
    pub frequency_mode: Option<String>,
}

/// Looks up each distinct character of `text` (Yomitan's findKanji), at
/// most `limit` entries within `max_bytes` serialized.
pub fn run_kanji_lookup(
    store: &Store,
    text: &str,
    limit: usize,
    max_bytes: usize,
) -> Result<KanjiLookupResponse, StoreError> {
    let dictionaries = EnabledDictionaries::from_store(store)?;
    let mut entries = find_kanji(store, text, &dictionaries, false)?;
    let mut truncated = entries.len() > limit;
    entries.truncate(limit);
    let budget = max_bytes.saturating_sub(ENVELOPE_ALLOWANCE);
    let mut result = Vec::with_capacity(entries.len());
    let mut used = 0usize;
    for entry in entries {
        let json = kanji_json(entry);
        let size = serde_json::to_vec(&json).map_or(usize::MAX, |v| v.len());
        if used.saturating_add(size) > budget {
            truncated = true;
            break;
        }
        used += size;
        result.push(json);
    }
    Ok(KanjiLookupResponse {
        entries: result,
        truncated,
    })
}

fn kanji_json(entry: KanjiEntry) -> KanjiEntryJson {
    let dictionary = DictionaryRefJson {
        id: entry.dictionary_id,
        title: entry.dictionary,
    };
    KanjiEntryJson {
        character: entry.character,
        onyomi: entry.onyomi,
        kunyomi: entry.kunyomi,
        tags: tags_json(entry.tags),
        stats: entry
            .stats
            .into_iter()
            .map(|(category, stats)| KanjiStatGroupJson {
                category,
                stats: stats
                    .into_iter()
                    .map(|s| KanjiStatJson {
                        name: s.name,
                        category: s.category,
                        notes: s.content,
                        order: s.order,
                        score: s.score,
                        value: s.value,
                    })
                    .collect(),
            })
            .collect(),
        meanings: entry.definitions,
        frequencies: entry
            .frequencies
            .into_iter()
            .map(|f| KanjiFrequencyJson {
                dictionary: DictionaryRefJson {
                    id: f.dictionary_id,
                    title: f.dictionary,
                },
                frequency: f.frequency,
                display_value: f.display_value,
                display_value_parsed: f.display_value_parsed,
                frequency_mode: f.frequency_mode,
            })
            .collect(),
        dictionary,
    }
}

/// The first `count` characters of `text`.
fn char_prefix(text: &str, count: usize) -> &str {
    match text.char_indices().nth(count) {
        Some((end, _)) => text.get(..end).unwrap_or(text),
        None => text,
    }
}

/// The longest prefix of `text` that is at most `units` UTF-16 code units.
fn utf16_prefix(text: &str, units: usize) -> &str {
    let mut used = 0;
    for (i, c) in text.char_indices() {
        if used + c.len_utf16() > units {
            return text.get(..i).unwrap_or_default();
        }
        used += c.len_utf16();
    }
    text
}

/// Response of a text parse.
#[derive(Debug, Clone, Serialize)]
pub struct ParseResponse {
    pub tokens: Vec<ParseTokenJson>,
}

/// One token: a word the lookup matched, or a run of unmatched text.
#[derive(Debug, Clone, Serialize)]
pub struct ParseTokenJson {
    pub text: String,
    /// The text in pieces with their readings (empty reading: read as
    /// written).
    pub segments: Vec<FuriganaJson>,
    /// Headwords whose lookup matched exactly this token, grouped per
    /// result entry in result order; empty for unmatched text.
    pub headwords: Vec<Vec<ParsedHeadwordJson>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuriganaJson {
    pub text: String,
    pub reading: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedHeadwordJson {
    pub term: String,
    pub reading: String,
}

/// Parses `text` with Yomitan's scanning parser, looking up `scan_length`
/// UTF-16 units from each position.
pub fn run_parse(
    store: &Store,
    text: &str,
    scan_length: usize,
) -> Result<ParseResponse, StoreError> {
    let dictionaries = EnabledDictionaries::from_store(store)?;
    let tokens = parse_text(store, text, &dictionaries, scan_length)?;
    Ok(ParseResponse {
        tokens: tokens
            .into_iter()
            .map(|token| ParseTokenJson {
                text: token.text(),
                segments: token
                    .segments
                    .into_iter()
                    .map(|s| FuriganaJson {
                        text: s.text,
                        reading: s.reading,
                    })
                    .collect(),
                headwords: token
                    .headwords
                    .into_iter()
                    .map(|list| {
                        list.into_iter()
                            .map(|h| ParsedHeadwordJson {
                                term: h.term,
                                reading: h.reading,
                            })
                            .collect()
                    })
                    .collect(),
            })
            .collect(),
    })
}
