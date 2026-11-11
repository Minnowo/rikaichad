// Ported from Yomitan ext/js/language/translator.js (_getTextVariants,
// _getProcessedTexts) at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2023-2026  Yomitan Authors
// Copyright (C) 2016-2022  Yomichan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Text variants from the preprocessor chain, with the chains of
//! preprocessor ids that produced each variant.

use std::collections::HashMap;

use rikaichad_ja::TextPreprocessor;

/// Chains of preprocessor ids, one per way a variant was produced.
pub type ProcessorChains = Vec<Vec<&'static str>>;

/// Memoized preprocessor outputs, shared across the prefixes of one lookup.
#[derive(Default)]
pub struct TextCache {
    outputs: HashMap<(String, &'static str), Vec<String>>,
}

/// Applies `processors` in order to `text`, keeping every distinct output
/// in first-seen order (like Yomitan's Map). A processor output equal to
/// its input keeps the chains unchanged; a new output gets the processor id
/// appended to each chain; outputs reached in several ways collect all
/// chains.
pub fn text_variants(
    text: &str,
    processors: &[TextPreprocessor],
    cache: &mut TextCache,
) -> Vec<(String, ProcessorChains)> {
    let mut variants = OrderedVariants::default();
    variants.set(text.to_string(), vec![Vec::new()]);
    for processor in processors {
        let mut next = OrderedVariants::default();
        for (variant, chains) in variants.entries {
            let outputs = cache
                .outputs
                .entry((variant.clone(), processor.id))
                .or_insert_with(|| (processor.process)(&variant))
                .clone();
            for processed in outputs {
                let existing = next.get(&processed).cloned();
                if processed == variant {
                    if existing.is_none() {
                        next.set(processed, chains.clone());
                    }
                } else {
                    let extended: ProcessorChains = chains
                        .iter()
                        .map(|chain| {
                            let mut chain = chain.clone();
                            chain.push(processor.id);
                            chain
                        })
                        .collect();
                    match existing {
                        None => next.set(processed, extended),
                        Some(mut all) => {
                            all.extend(extended);
                            next.set(processed, all);
                        }
                    }
                }
            }
        }
        variants = next;
    }
    variants.entries
}

/// An insertion-ordered map from variant text to chains; setting an
/// existing key keeps its position, as with a JavaScript Map.
#[derive(Default)]
struct OrderedVariants {
    entries: Vec<(String, ProcessorChains)>,
    positions: HashMap<String, usize>,
}

impl OrderedVariants {
    fn get(&self, key: &str) -> Option<&ProcessorChains> {
        let position = *self.positions.get(key)?;
        self.entries.get(position).map(|entry| &entry.1)
    }

    fn set(&mut self, key: String, value: ProcessorChains) {
        if let Some(&position) = self.positions.get(&key) {
            if let Some(entry) = self.entries.get_mut(position) {
                entry.1 = value;
            }
            return;
        }
        self.positions.insert(key.clone(), self.entries.len());
        self.entries.push((key, value));
    }
}
