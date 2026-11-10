// SPDX-License-Identifier: AGPL-3.0-or-later
//! Small hand-written dictionaries for tests. All content here is written
//! for this project (no third-party dictionary data).

use crate::ZipBuilder;

/// Builds a dictionary zip from `index.json` text and `(name, contents)`
/// files, all deflate-compressed, in the given order.
pub fn dictionary(index: &str, files: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = ZipBuilder::new();
    zip.add("index.json", index.as_bytes());
    for (name, contents) in files {
        zip.add(name, contents.as_bytes());
    }
    zip.finish()
}

/// `index.json` text for a format-3 dictionary titled `title`.
pub fn index(title: &str) -> String {
    format!(r#"{{"title":"{title}","revision":"1","format":3,"sequenced":true}}"#)
}

/// A small term dictionary with plain, text-object and structured-content
/// glossaries, an empty reading, tags and two term banks.
pub fn terms_dictionary(title: &str) -> Vec<u8> {
    dictionary(
        &index(title),
        &[
            (
                "term_bank_1.json",
                r#"[
                ["食べる","たべる","v1 vt","v1",10,["to eat",{"type":"text","text":"to consume"}],100,"P"],
                ["食う","くう","v5","v5",0,[{"type":"structured-content","content":{"tag":"ul","content":[{"tag":"li","content":"to eat (rough)"},{"tag":"li","content":["to ",{"tag":"ruby","content":["喰",{"tag":"rt","content":"く"}]},"う"]}]}}],101,""]
                ]"#,
            ),
            (
                "term_bank_2.json",
                r#"[["ご飯","ごはん","n",null,5,["cooked rice","meal"],102,"P"],["カタカナ","","n","",0,["katakana"],103,""]]"#,
            ),
            (
                "tag_bank_1.json",
                r#"[["P","popular",-10,"common word",10],["v1","partOfSpeech",0,"ichidan verb",0]]"#,
            ),
            (
                "term_meta_bank_1.json",
                r#"[["食べる","freq",{"value":250,"displayValue":"250"}],["ご飯","pitch",{"reading":"ごはん","pitches":[{"position":1}]}]]"#,
            ),
            (
                "kanji_bank_1.json",
                r#"[["食","ショク","た.べる","",["eat"],{"strokes":"9"}]]"#,
            ),
            ("kanji_meta_bank_1.json", r#"[["食","freq",328]]"#),
            ("styles.css", ".x { color: red; }"),
            ("img/ignored.png", "not really a png"),
        ],
    )
}
