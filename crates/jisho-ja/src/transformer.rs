// Ported from Yomitan ext/js/language/language-transformer.js and
// ext/js/language/language-transforms.js at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2024-2026  Yomitan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Rule-based deinflection (Yomitan's LanguageTransformer) over the
//! Japanese rule table.
//!
//! [`transform`] returns every text reachable by undoing inflections,
//! breadth first, each with the conditions it now carries and the trace of
//! transforms applied (newest first). A candidate is only valid for a
//! dictionary entry whose part-of-speech flags pass [`conditions_match`].

use crate::data::{CONDITIONS, TRANSFORMS};

/// Safety cap on candidates per input. Yomitan has none; real Japanese
/// text produces at most a few hundred, so this only stops pathological
/// input sent to the HTTP API from running away.
const MAX_RESULTS: usize = 20_000;

/// A deinflection candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformedText {
    /// The candidate text.
    pub text: String,
    /// Conditions the candidate carries (0 for the unchanged input).
    pub conditions: u32,
    /// Transforms applied, newest first (Yomitan's trace order).
    pub trace: Vec<TraceFrame>,
}

/// One applied rule in a trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceFrame {
    /// Transform id, e.g. "-te" in kana.
    pub transform: &'static str,
    /// Index of the rule within the transform.
    pub rule_index: usize,
    /// Text the rule was applied to.
    pub text: String,
}

/// All deinflection candidates of `source`, starting with `source` itself
/// (conditions 0, empty trace), in Yomitan's breadth-first order.
pub fn transform(source: &str) -> Vec<TransformedText> {
    let mut results = vec![TransformedText {
        text: source.to_string(),
        conditions: 0,
        trace: Vec::new(),
    }];
    let mut i = 0;
    while i < results.len() && results.len() < MAX_RESULTS {
        let Some(current) = results.get(i).cloned() else {
            break;
        };
        i += 1;
        for transform in TRANSFORMS {
            for (j, rule) in transform.rules.iter().enumerate() {
                if !conditions_match(current.conditions, rule.conditions_in) {
                    continue;
                }
                let deinflected = if rule.whole_word {
                    if current.text != rule.inflected {
                        continue;
                    }
                    rule.deinflected.to_string()
                } else {
                    let Some(stem) = current.text.strip_suffix(rule.inflected) else {
                        continue;
                    };
                    format!("{stem}{}", rule.deinflected)
                };
                let is_cycle = current.trace.iter().any(|frame| {
                    frame.transform == transform.id
                        && frame.rule_index == j
                        && frame.text == current.text
                });
                if is_cycle {
                    continue;
                }
                let mut trace = Vec::with_capacity(current.trace.len() + 1);
                trace.push(TraceFrame {
                    transform: transform.id,
                    rule_index: j,
                    text: current.text.clone(),
                });
                trace.extend(current.trace.iter().cloned());
                results.push(TransformedText {
                    text: deinflected,
                    conditions: rule.conditions_out,
                    trace,
                });
            }
        }
    }
    results
}

/// Yomitan's LanguageTransformer.conditionsMatch: true if `current` is 0
/// (unconditioned) or shares a bit with `next`.
pub fn conditions_match(current: u32, next: u32) -> bool {
    current == 0 || (current & next) != 0
}

/// Flags of the dictionary-form conditions among `parts_of_speech` (a
/// dictionary entry's `rules` field). Unknown names contribute nothing.
pub fn condition_flags_from_parts_of_speech<S: AsRef<str>>(parts_of_speech: &[S]) -> u32 {
    let mut flags = 0;
    for part in parts_of_speech {
        let part = part.as_ref();
        if let Some(condition) = CONDITIONS
            .iter()
            .find(|c| c.is_dictionary_form && c.id == part)
        {
            flags |= condition.flags;
        }
    }
    flags
}

/// Flags of one condition type (any condition, not only dictionary forms);
/// 0 if unknown.
pub fn condition_flags_from_condition_type(condition_type: &str) -> u32 {
    CONDITIONS
        .iter()
        .find(|c| c.id == condition_type)
        .map_or(0, |c| c.flags)
}

/// The user-facing name and description of a transform id
/// (getUserFacingInflectionRules); unknown ids are returned as their own
/// name with no description.
pub fn user_facing_rule(id: &str) -> (String, Option<&'static str>) {
    match TRANSFORMS.iter().find(|t| t.id == id) {
        Some(t) => (t.name.to_string(), t.description.filter(|d| !d.is_empty())),
        None => (id.to_string(), None),
    }
}
