// SPDX-License-Identifier: AGPL-3.0-or-later
//! The string comparison Yomitan's translator uses for its final
//! tie-breaks: `Intl.Collator('en-US')`, ICU's CLDR root collation.
//!
//! jisho compares by per-character weights measured from that collator
//! (data/collation.json, made by scripts/port/dump-collation.mjs): all
//! primary weights first (the letter: kana of either script, with or
//! without dakuten, share one), then secondary (dakuten, accents), then
//! tertiary (case, hiragana before katakana, width). This matches the
//! collator for the characters in the table: the Basic Multilingual Plane
//! except characters the collator expands to several (e.g. U+3006,
//! U+33E0) or that only carry accents (combining marks), which are rare in
//! headwords and tag names. A character outside the table gets a primary weight after every
//! character in it, in code point order, so the comparison stays a total
//! order (docs/brief.md section 5).

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Collation weights of a character: (primary, secondary, tertiary) ranks;
/// (0, 0, 0) for one that is ignored (variation selectors, zero-width
/// marks).
type Weights = (u32, u32, u32);

const IGNORED: Weights = (0, 0, 0);

const TABLE_JSON: &str = include_str!("../data/collation.json");

fn table() -> &'static HashMap<char, Weights> {
    static TABLE: OnceLock<HashMap<char, Weights>> = OnceLock::new();
    TABLE.get_or_init(|| build_table(TABLE_JSON))
}

/// Builds the weights from the dumped sorted characters and their level
/// digits. An invalid table yields an empty map (every comparison then
/// falls back to code points); the tests check the shipped table.
fn build_table(json: &str) -> HashMap<char, Weights> {
    let Ok(dump) = serde_json::from_str::<serde_json::Value>(json) else {
        return HashMap::new();
    };
    let (Some(chars), Some(levels), Some(ignorable)) = (
        dump.get("chars").and_then(serde_json::Value::as_str),
        dump.get("levels").and_then(serde_json::Value::as_str),
        dump.get("ignorable").and_then(serde_json::Value::as_str),
    ) else {
        return HashMap::new();
    };
    let mut map: HashMap<char, Weights> = ignorable.chars().map(|c| (c, IGNORED)).collect();
    let (mut primary, mut secondary, mut tertiary) = (0u32, 0u32, 0u32);
    for (c, level) in chars.chars().zip(levels.bytes()) {
        if level >= b'3' {
            primary += 1;
        }
        if level >= b'2' {
            secondary += 1;
        }
        if level >= b'1' {
            tertiary += 1;
        }
        map.insert(c, (primary, secondary, tertiary));
    }
    map
}

/// Compares like Yomitan's string comparer (see the module docs).
pub fn compare(a: &str, b: &str) -> Ordering {
    let table = table();
    let weights = |s| weights(table, s);
    weights(a)
        .map(|w| w.0)
        .cmp(weights(b).map(|w| w.0))
        .then_with(|| weights(a).map(|w| w.1).cmp(weights(b).map(|w| w.1)))
        .then_with(|| weights(a).map(|w| w.2).cmp(weights(b).map(|w| w.2)))
}

/// Primary weights of characters outside the table start here (the table
/// has about 56,000), followed by the code point.
const UNKNOWN_BASE: u32 = 1 << 20;

/// The weights of the characters of `s` that are not ignored.
fn weights<'a>(
    table: &'a HashMap<char, Weights>,
    s: &'a str,
) -> impl Iterator<Item = Weights> + 'a {
    s.chars()
        .map(|c| {
            table
                .get(&c)
                .copied()
                .unwrap_or((UNKNOWN_BASE + u32::from(c), 0, 0))
        })
        .filter(|w| *w != IGNORED)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn table_is_complete() {
        let dump: serde_json::Value = serde_json::from_str(TABLE_JSON).unwrap();
        let chars = dump["chars"].as_str().unwrap().chars().count();
        let ignorable = dump["ignorable"].as_str().unwrap().chars().count();
        assert!(ignorable > 0);
        assert_eq!(table().len(), chars + ignorable);
        assert_eq!(dump["levels"].as_str().unwrap().len(), chars);
    }

    #[test]
    fn matches_intl_collator() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/collator-golden.json"
        ))
        .unwrap();
        let golden: serde_json::Value = serde_json::from_str(&text).unwrap();
        let all: Vec<String> = serde_json::from_value(golden["sorted"].clone()).unwrap();
        // Strings with a character outside the table are not in the
        // collator's order.
        let expected: Vec<String> = all
            .into_iter()
            .filter(|s| s.chars().all(|c| table().contains_key(&c)))
            .collect();
        assert!(expected.len() > 5000, "{}", expected.len());
        let mut actual = expected.clone();
        actual.reverse();
        actual.sort_by(|a, b| compare(a, b));
        assert_eq!(actual, expected);
    }

    #[test]
    fn unknown_characters_keep_a_total_order() {
        // U+20B9F is outside the table, U+3006 is left out (an expansion).
        let mut words = vec!["\u{20b9f}", "b", "\u{3006}", "a\u{20b9f}", "ab", "\u{3042}"];
        words.sort_by(|a, b| compare(a, b));
        assert_eq!(
            words,
            vec!["ab", "a\u{20b9f}", "b", "\u{3042}", "\u{3006}", "\u{20b9f}"]
        );
        for a in &words {
            for b in &words {
                assert_eq!(compare(a, b), compare(b, a).reverse(), "{a} {b}");
            }
        }
    }

    #[test]
    fn case_kana_and_prefix_order() {
        let mut words = vec![
            "ab ", "Ab", "aB", "ab", "a", "A", "a-b", "a b", "a1", "P", "news1",
        ];
        words.sort_by(|a, b| compare(a, b));
        assert_eq!(
            words,
            vec![
                "a", "A", "a b", "a-b", "a1", "ab", "aB", "Ab", "ab ", "news1", "P"
            ]
        );
        // Kana compare by letter first (ki < ku whatever the script), then
        // dakuten (secondary), then script (tertiary): hiragana kaki,
        // katakana kaku, hiragana kagu (as Intl.Collator).
        let mut kana = vec!["\u{304b}\u{3050}", "\u{30ab}\u{30af}", "\u{304b}\u{304d}"];
        kana.sort_by(|a, b| compare(a, b));
        assert_eq!(
            kana,
            vec!["\u{304b}\u{304d}", "\u{30ab}\u{30af}", "\u{304b}\u{3050}"]
        );
    }
}
