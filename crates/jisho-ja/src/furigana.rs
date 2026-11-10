// Ported from Yomitan ext/js/language/ja/japanese.js (distributeFurigana,
// distributeFuriganaInflected, segmentizeFurigana, getFuriganaKanaSegments,
// getStemLength, createFuriganaSegment) at
// ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Furigana: splitting a term into pieces with the reading of each (kana
//! pieces get no reading). Like Yomitan, all positions are UTF-16 code
//! units, so the work happens on UTF-16 buffers.

use crate::kana::{convert_katakana_to_hiragana, is_code_point_kana};

/// A piece of a term and its reading (empty when the piece is read as
/// written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuriganaSegment {
    pub text: String,
    pub reading: String,
}

/// UTF-16 code units of a string.
type Units = Vec<u16>;

#[derive(Debug, Clone)]
struct Segment {
    text: Units,
    reading: Units,
}

struct Group {
    is_kana: bool,
    text: Units,
    normalized: Option<Units>,
}

/// distributeFurigana: the pieces of `term` with their readings, or the
/// whole term with the whole reading when they cannot be matched up
/// unambiguously.
pub fn distribute_furigana(term: &str, reading: &str) -> Vec<FuriganaSegment> {
    finish(distribute(&units(term), &units(reading)))
}

/// distributeFuriganaInflected: like [`distribute_furigana`] for an
/// inflected `source` of the term (e.g. the text a lookup matched): the
/// common stem is split as the term, the inflected rest has no reading.
pub fn distribute_furigana_inflected(
    term: &str,
    reading: &str,
    source: &str,
) -> Vec<FuriganaSegment> {
    let term_normalized = units(&convert_katakana_to_hiragana(term, false));
    let reading_normalized = units(&convert_katakana_to_hiragana(reading, false));
    let source_normalized = units(&convert_katakana_to_hiragana(source, false));
    let source = units(source);
    let mut reading = units(reading);

    let mut main_text = units(term);
    let mut stem_length = common_stem_length(&term_normalized, &source_normalized);

    // Is the source derived from the reading instead of the term?
    let reading_stem_length = common_stem_length(&reading_normalized, &source_normalized);
    if reading_stem_length > 0 && reading_stem_length >= stem_length {
        main_text.clone_from(&reading);
        stem_length = reading_stem_length;
        reading = concat(
            substring(&source, 0, stem_length),
            from(&reading, stem_length),
        );
    }

    let mut segments: Vec<Segment> = Vec::new();
    if stem_length > 0 {
        main_text = concat(
            substring(&source, 0, stem_length),
            from(&main_text, stem_length),
        );
        let mut consumed = 0;
        for segment in distribute(&main_text, &reading) {
            let start = consumed;
            consumed += segment.text.len();
            if consumed < stem_length {
                segments.push(segment);
            } else if consumed == stem_length {
                segments.push(segment);
                break;
            } else {
                if start < stem_length {
                    segments.push(Segment {
                        text: substring(&main_text, start, stem_length).to_vec(),
                        reading: Vec::new(),
                    });
                }
                break;
            }
        }
    }

    if stem_length < source.len() {
        let remainder = from(&source, stem_length);
        match segments.last_mut() {
            // Append to the last segment if it has an empty reading.
            Some(last) if last.reading.is_empty() => last.text.extend_from_slice(remainder),
            _ => segments.push(Segment {
                text: remainder.to_vec(),
                reading: Vec::new(),
            }),
        }
    }
    finish(segments)
}

/// getStemLength: length of the common prefix in UTF-16 units, never
/// ending inside a surrogate pair.
fn common_stem_length(text1: &[u16], text2: &[u16]) -> usize {
    let min_length = text1.len().min(text2.len());
    if min_length == 0 {
        return 0;
    }
    let mut i = 0;
    loop {
        let (char1, length) = code_point_at(text1, i);
        let (char2, _) = code_point_at(text2, i);
        if char1 != char2 {
            break;
        }
        i += length;
        if i >= min_length {
            if i > min_length {
                // Don't consume partial UTF-16 surrogate characters.
                i -= length;
            }
            break;
        }
    }
    i
}

/// String.codePointAt: the code point at UTF-16 index `i` and its length
/// in units (1 past the end, as an impossible value, when out of range).
fn code_point_at(text: &[u16], i: usize) -> (u32, usize) {
    match (text.get(i), text.get(i + 1)) {
        (Some(&high), Some(&low))
            if (0xd800..0xdc00).contains(&high) && (0xdc00..0xe000).contains(&low) =>
        {
            let code_point =
                0x10000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00);
            (code_point, 2)
        }
        (Some(&unit), _) => (u32::from(unit), 1),
        (None, _) => (u32::MAX, 1),
    }
}

/// distributeFurigana on UTF-16 buffers.
fn distribute(term: &[u16], reading: &[u16]) -> Vec<Segment> {
    if reading == term {
        return vec![Segment {
            text: term.to_vec(),
            reading: Vec::new(),
        }];
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut i = 0;
    while i < term.len() {
        let (code_point, length) = code_point_at(term, i);
        let c = substring(term, i, i + length);
        let is_kana = is_code_point_kana(code_point);
        match groups.last_mut() {
            Some(group) if group.is_kana == is_kana => group.text.extend_from_slice(c),
            _ => groups.push(Group {
                is_kana,
                text: c.to_vec(),
                normalized: None,
            }),
        }
        i += length;
    }
    for group in &mut groups {
        if group.is_kana {
            let text = String::from_utf16_lossy(&group.text);
            group.normalized = Some(units(&convert_katakana_to_hiragana(&text, false)));
        }
    }
    let reading_normalized = units(&convert_katakana_to_hiragana(
        &String::from_utf16_lossy(reading),
        false,
    ));
    segmentize(reading, &reading_normalized, &groups).unwrap_or_else(|| {
        vec![Segment {
            text: term.to_vec(),
            reading: reading.to_vec(),
        }]
    })
}

/// segmentizeFurigana: `None` when the groups cannot take the reading, or
/// can in more than one way.
fn segmentize(
    reading: &[u16],
    reading_normalized: &[u16],
    groups: &[Group],
) -> Option<Vec<Segment>> {
    let Some((group, rest)) = groups.split_first() else {
        return reading.is_empty().then(Vec::new);
    };
    let text_length = group.text.len();
    if group.is_kana {
        let normalized = group.normalized.as_ref()?;
        if !reading_normalized.starts_with(normalized) {
            return None;
        }
        let mut segments = segmentize(
            from(reading, text_length),
            from(reading_normalized, text_length),
            rest,
        )?;
        if reading.starts_with(&group.text) {
            segments.insert(
                0,
                Segment {
                    text: group.text.clone(),
                    reading: Vec::new(),
                },
            );
        } else {
            let kana = kana_segments(&group.text, reading);
            segments.splice(0..0, kana);
        }
        return Some(segments);
    }
    let mut result: Option<Vec<Segment>> = None;
    let mut i = reading.len();
    while i >= text_length {
        if let Some(mut segments) = segmentize(from(reading, i), from(reading_normalized, i), rest)
        {
            if result.is_some() {
                // More than one way to segmentize the tail: ambiguous.
                return None;
            }
            segments.insert(
                0,
                Segment {
                    text: group.text.clone(),
                    reading: substring(reading, 0, i).to_vec(),
                },
            );
            result = Some(segments);
        }
        // There is only one way to segmentize the last non-kana group.
        if groups.len() == 1 || i == 0 {
            break;
        }
        i -= 1;
    }
    result
}

/// getFuriganaKanaSegments: a kana group read differently from how it is
/// written (e.g. katakana in the term, hiragana in the reading), split
/// into runs that match the reading and runs that do not.
fn kana_segments(text: &[u16], reading: &[u16]) -> Vec<Segment> {
    let same = |i: usize| match (reading.get(i), text.get(i)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    let piece = |start: usize, end: usize, state: bool| Segment {
        text: substring(text, start, end).to_vec(),
        reading: if state {
            Vec::new()
        } else {
            substring(reading, start, end).to_vec()
        },
    };
    let mut segments = Vec::new();
    let mut start = 0;
    let mut state = same(0);
    for i in 1..text.len() {
        let new_state = same(i);
        if state == new_state {
            continue;
        }
        segments.push(piece(start, i, state));
        state = new_state;
        start = i;
    }
    segments.push(piece(start, text.len(), state));
    segments
}

fn units(text: &str) -> Units {
    text.encode_utf16().collect()
}

/// String.substring(start, end) with JavaScript's clamping (start <= end).
fn substring(text: &[u16], start: usize, end: usize) -> &[u16] {
    let end = end.min(text.len());
    let start = start.min(end);
    text.get(start..end).unwrap_or_default()
}

/// String.substring(start).
fn from(text: &[u16], start: usize) -> &[u16] {
    substring(text, start, text.len())
}

fn concat(a: &[u16], b: &[u16]) -> Units {
    let mut out = a.to_vec();
    out.extend_from_slice(b);
    out
}

fn finish(segments: Vec<Segment>) -> Vec<FuriganaSegment> {
    segments
        .into_iter()
        .map(|s| FuriganaSegment {
            text: String::from_utf16_lossy(&s.text),
            reading: String::from_utf16_lossy(&s.reading),
        })
        .collect()
}
