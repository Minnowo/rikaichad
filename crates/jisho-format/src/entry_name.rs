// SPDX-License-Identifier: AGPL-3.0-or-later
//! Classification of archive entry names into dictionary file kinds.

/// Name of the dictionary index file.
pub const INDEX_FILE_NAME: &str = "index.json";

/// Name of the optional dictionary stylesheet.
pub const STYLES_FILE_NAME: &str = "styles.css";

/// What an archive entry is, judged by its exact name.
///
/// Matches Yomitan's importer: only root-level names count, bank files match
/// `^<prefix>_(\d+)\.json$`, everything else (media, `__MACOSX/`, nested
/// files) is [`EntryKind::Other`] and ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Index,
    Styles,
    TermBank,
    TermMetaBank,
    KanjiBank,
    KanjiMetaBank,
    TagBank,
    Other,
}

/// Classifies an archive entry name.
pub fn classify(name: &str) -> EntryKind {
    if name == INDEX_FILE_NAME {
        return EntryKind::Index;
    }
    if name == STYLES_FILE_NAME {
        return EntryKind::Styles;
    }
    // Longer prefixes first: "term_meta_bank_" must not be read as "term_".
    let banks = [
        ("term_meta_bank_", EntryKind::TermMetaBank),
        ("kanji_meta_bank_", EntryKind::KanjiMetaBank),
        ("term_bank_", EntryKind::TermBank),
        ("kanji_bank_", EntryKind::KanjiBank),
        ("tag_bank_", EntryKind::TagBank),
    ];
    for (prefix, kind) in banks {
        if let Some(number) = name
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(".json"))
            && !number.is_empty()
            && number.bytes().all(|b| b.is_ascii_digit())
        {
            return kind;
        }
    }
    EntryKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_root_level_names_only() {
        assert_eq!(classify("index.json"), EntryKind::Index);
        assert_eq!(classify("styles.css"), EntryKind::Styles);
        assert_eq!(classify("term_bank_1.json"), EntryKind::TermBank);
        assert_eq!(classify("term_meta_bank_12.json"), EntryKind::TermMetaBank);
        assert_eq!(classify("kanji_bank_3.json"), EntryKind::KanjiBank);
        assert_eq!(classify("kanji_meta_bank_3.json"), EntryKind::KanjiMetaBank);
        assert_eq!(classify("tag_bank_1.json"), EntryKind::TagBank);
    }

    #[test]
    fn ignores_nested_junk_and_malformed_names() {
        assert_eq!(classify("__MACOSX/._index.json"), EntryKind::Other);
        assert_eq!(classify("dict/index.json"), EntryKind::Other);
        assert_eq!(classify("dict/term_bank_1.json"), EntryKind::Other);
        assert_eq!(classify("term_bank_.json"), EntryKind::Other);
        assert_eq!(classify("term_bank_1a.json"), EntryKind::Other);
        assert_eq!(classify("term_bank_1.json.bak"), EntryKind::Other);
        assert_eq!(classify("img/a.png"), EntryKind::Other);
    }
}
