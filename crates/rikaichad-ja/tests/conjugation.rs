// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conjugation tables: exact forms for regular and irregular words, and
//! every form deinflects back to its dictionary form, apart from the
//! exceptions listed here (each tied to the cells it covers).

use rikaichad_ja::{
    Conjugation, FORM_NAMES, Form, condition_flags_from_parts_of_speech, conditions_match,
    conjugate, transform,
};

/// (term, reading, word class): words covering every class and every
/// irregular case the table handles.
const WORDS: &[(&str, &str, &str)] = &[
    ("食べる", "たべる", "v1"),
    ("見る", "みる", "v1"),
    ("くれる", "", "v1"),
    ("買う", "かう", "v5"),
    ("書く", "かく", "v5"),
    ("泳ぐ", "およぐ", "v5"),
    ("話す", "はなす", "v5"),
    ("待つ", "まつ", "v5"),
    ("死ぬ", "しぬ", "v5"),
    ("遊ぶ", "あそぶ", "v5"),
    ("読む", "よむ", "v5"),
    ("帰る", "かえる", "v5"),
    ("まとう", "", "v5"),
    ("行く", "いく", "v5"),
    ("行く", "ゆく", "v5"),
    ("出ていく", "でていく", "v5"),
    ("ある", "ある", "v5"),
    ("有る", "ある", "v5"),
    ("問う", "とう", "v5"),
    ("恋う", "こう", "v5"),
    ("宣う", "のたまう", "v5"),
    ("いらっしゃる", "", "v5"),
    ("おっしゃる", "", "v5"),
    ("下さる", "くださる", "v5"),
    ("なさる", "", "v5"),
    ("ござる", "", "v5"),
    ("お越しくださる", "おこしくださる", "v5"),
    ("来る", "くる", "vk"),
    ("くる", "", "vk"),
    ("持って来る", "もってくる", "vk"),
    ("する", "する", "vs"),
    ("為る", "する", "vs"),
    ("勉強", "べんきょう", "vs"),
    ("勉強する", "べんきょうする", "vs"),
    ("高い", "たかい", "adj-i"),
    ("かわいい", "", "adj-i"),
    ("可愛い", "かわいい", "adj-i"),
    ("いい", "いい", "adj-i"),
    ("良い", "いい", "adj-i"),
    ("かっこいい", "", "adj-i"),
];

/// Bare honorific verbs; Yomitan deinflects their -imasu forms only as
/// whole words.
const HONORIFICS: &[&str] = &["いらっしゃる", "ござる", "なさる", "くださる", "おっしゃる"];

/// Texts to deinflect and accepted (dictionary text, word class) targets.
type Expectation = (Vec<String>, Vec<(String, &'static str)>);

/// What `text` (a form's term or reading) is checked against: the texts to
/// deinflect (the form, or the form without a separate word at its end)
/// and the accepted targets (dictionary text, word class). None means the
/// form is not checked. Each exception is tied to the cells it covers.
fn expectation(
    table: &Conjugation,
    form: &Form,
    text: &str,
    expected: &str,
) -> Option<Expectation> {
    let cell = (form.name, form.polite, form.negative);
    let mut texts = vec![text.to_string()];
    // Separate words ending some forms: -naide (de), the prohibitive -na,
    // polite -desu. Lookup finds the form before them.
    let tail = match cell {
        ("te", false, true) => Some("で"),
        ("imperative", false, true) => Some("な"),
        ("desire", true, _) => Some("です"),
        ("non-past" | "past", true, _) if table.word_class == "adj-i" => Some("です"),
        _ => None,
    };
    if let Some(head) = tail.and_then(|t| text.strip_suffix(t)) {
        texts.push(head.to_string());
    }
    let mut targets = vec![(expected.to_string(), table.word_class)];
    // aru's negatives are the adjective nai.
    if table.reading == "ある" && form.negative {
        targets.push(("ない".to_string(), "adj-i"));
    }
    // ii's forms are found under yoi.
    if table.reading.ends_with("いい") && text != expected && !text.starts_with(expected) {
        let reading_prefix = table.reading.strip_suffix("いい")?;
        targets.push((format!("{reading_prefix}よい"), "adj-i"));
        if let Some(term_prefix) = table.term.strip_suffix("いい") {
            targets.push((format!("{term_prefix}よい"), "adj-i"));
        }
    }
    // yuku's -te/-ta forms (and -te iru) read itte/itta, found under iku.
    if let Some(prefix) = table.reading.strip_suffix("ゆく")
        && matches!(
            cell,
            ("te" | "past" | "tara", false, false) | ("progressive", _, _)
        )
        && text.starts_with(&format!("{prefix}いっ"))
    {
        targets.push((format!("{prefix}いく"), "v5"));
    }
    // suru's potential is the separate verb dekiru.
    if cell.0 == "potential" && table.word_class == "vs" {
        let stem = expected.strip_suffix("する")?;
        texts.push(text.strip_prefix(stem)?.to_string());
        targets.push(("できる".to_string(), "v1"));
    }
    // Honorific -i forms: Yomitan has no rule for the imperative
    // (irasshai), and deinflects -imasu only for the bare verbs.
    let honorific = HONORIFICS.iter().any(|h| table.reading.ends_with(h));
    if honorific {
        let bare = HONORIFICS.contains(&table.reading.as_str());
        let imperative = cell == ("imperative", false, false);
        let masu = form.polite && text.contains("いま");
        if imperative || (masu && !bare) {
            return None;
        }
    }
    Some((texts, targets))
}

#[test]
fn every_form_deinflects_to_the_dictionary_form() {
    let mut failures = Vec::new();
    for &(term, reading, class) in WORDS {
        let table = conjugate(term, reading, &[class]).unwrap_or_else(|| panic!("{term}"));
        for form in &table.forms {
            for (text, expected) in [(&form.term, &table.term), (&form.reading, &table.reading)] {
                let Some((texts, targets)) = expectation(&table, form, text, expected) else {
                    continue;
                };
                if !deinflects(&texts, &targets) {
                    failures.push(format!("{text} ({} of {term}) -> {expected}", form.name));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "not deinflected:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_form_name_is_listed() {
    for &(term, reading, class) in WORDS {
        let table = conjugate(term, reading, &[class]).unwrap_or_else(|| panic!("{term}"));
        for form in &table.forms {
            assert!(FORM_NAMES.contains(&form.name), "{}", form.name);
        }
    }
}

#[test]
fn ichidan_forms() {
    let table = conjugate("食べる", "たべる", &["v1", "vt"]).unwrap();
    assert_eq!(table.word_class, "v1");
    assert_eq!(
        cell(&table, "non-past"),
        ["食べる", "食べない", "食べます", "食べません"]
    );
    assert_eq!(cell(&table, "te"), ["食べて", "食べなくて", "食べないで"]);
    assert_eq!(
        cell(&table, "imperative"),
        ["食べろ", "食べるな", "食べなさい"]
    );
    assert_eq!(cell(&table, "potential")[0], "食べられる");
    assert_eq!(cell(&table, "causative-passive")[0], "食べさせられる");
    assert_eq!(readings(&table, "volitional"), ["たべよう", "たべましょう"]);
    let kureru = conjugate("くれる", "", &["v1"]).unwrap();
    assert_eq!(cell(&kureru, "imperative")[0], "くれ");
}

#[test]
fn godan_forms() {
    let table = conjugate("書く", "かく", &["v5"]).unwrap();
    assert_eq!(
        cell(&table, "past"),
        ["書いた", "書かなかった", "書きました", "書きませんでした"]
    );
    assert_eq!(cell(&table, "conditional"), ["書けば", "書かなければ"]);
    assert_eq!(cell(&table, "potential")[0], "書ける");
    assert_eq!(cell(&table, "passive")[0], "書かれる");
    assert_eq!(cell(&table, "volitional")[0], "書こう");
    assert_eq!(cell(&table, "progressive")[0], "書いている");

    let te = |term: &str, reading: &str| {
        cell(&conjugate(term, reading, &["v5"]).unwrap(), "te")[0].clone()
    };
    assert_eq!(te("買う", "かう"), "買って");
    assert_eq!(te("泳ぐ", "およぐ"), "泳いで");
    assert_eq!(te("話す", "はなす"), "話して");
    assert_eq!(te("死ぬ", "しぬ"), "死んで");
    assert_eq!(te("帰る", "かえる"), "帰って");
    assert_eq!(te("まとう", ""), "まとって");
}

#[test]
fn irregular_godan_verbs() {
    let iku = conjugate("行く", "いく", &["v5"]).unwrap();
    assert_eq!(cell(&iku, "past")[0], "行った");
    assert_eq!(cell(&iku, "te")[0], "行って");
    let yuku = conjugate("行く", "ゆく", &["v5"]).unwrap();
    assert_eq!(readings(&yuku, "te")[0], "いって");
    assert_eq!(readings(&yuku, "non-past")[1], "ゆかない");
    assert_eq!(
        cell(&conjugate("出ていく", "", &["v5"]).unwrap(), "te")[0],
        "出ていって"
    );

    for (term, reading) in [("ある", ""), ("有る", "ある")] {
        let aru = conjugate(term, reading, &["v5"]).unwrap();
        assert_eq!(
            cell(&aru, "non-past"),
            [
                term,
                "ない",
                &format!("{}ります", &term[..term.len() - "る".len()]),
                &format!("{}りません", &term[..term.len() - "る".len()])
            ]
        );
        assert_eq!(cell(&aru, "past")[1], "なかった");
        assert_eq!(cell(&aru, "te")[1..], ["なくて", "ないで"]);
        assert!(cell(&aru, "progressive").is_empty());
        assert!(cell(&aru, "potential").is_empty());
        assert!(cell(&aru, "passive").is_empty());
    }

    let tou = conjugate("問う", "とう", &["v5"]).unwrap();
    assert_eq!(cell(&tou, "te")[0], "問うて");
    assert_eq!(cell(&tou, "past")[0], "問うた");
    assert_eq!(
        cell(&conjugate("恋う", "こう", &["v5"]).unwrap(), "te")[0],
        "恋うて"
    );
    let notamau = conjugate("宣う", "のたまう", &["v5"]).unwrap();
    assert_eq!(cell(&notamau, "te")[0], "宣うて");
    assert_eq!(readings(&notamau, "te")[0], "のたもうて");
}

#[test]
fn honorific_verbs() {
    for (verb, imperative) in [
        ("いらっしゃる", Some("いらっしゃい")),
        ("おっしゃる", Some("おっしゃい")),
        ("くださる", Some("ください")),
        ("なさる", Some("なさい")),
        ("ござる", None),
    ] {
        let table = conjugate(verb, "", &["v5"]).unwrap();
        let stem = verb.strip_suffix('る').unwrap();
        assert_eq!(cell(&table, "non-past")[2], format!("{stem}います"));
        assert_eq!(cell(&table, "past")[2], format!("{stem}いました"));
        assert_eq!(cell(&table, "te")[0], format!("{stem}って"));
        let plain = forms(&table, "imperative", false, false);
        assert_eq!(plain.first().map(String::as_str), imperative, "{verb}");
        assert!(
            forms(&table, "imperative", true, false).is_empty(),
            "{verb}"
        );
        assert!(cell(&table, "desire").is_empty(), "{verb}");
    }
    let compound = conjugate("お越しくださる", "おこしくださる", &["v5"]).unwrap();
    assert_eq!(cell(&compound, "non-past")[2], "お越しくださいます");
}

#[test]
fn kuru_and_suru() {
    let kuru = conjugate("来る", "くる", &["vk"]).unwrap();
    assert_eq!(cell(&kuru, "non-past")[1], "来ない");
    assert_eq!(readings(&kuru, "non-past")[1], "こない");
    assert_eq!(readings(&kuru, "te")[0], "きて");
    assert_eq!(readings(&kuru, "imperative")[0], "こい");
    assert_eq!(readings(&kuru, "conditional")[0], "くれば");
    let compound = conjugate("持って来る", "もってくる", &["vk"]).unwrap();
    assert_eq!(cell(&compound, "past")[0], "持って来た");
    assert_eq!(readings(&compound, "past")[0], "もってきた");

    let suru = conjugate("する", "する", &["vs"]).unwrap();
    assert_eq!(
        cell(&suru, "non-past"),
        ["する", "しない", "します", "しません"]
    );
    assert_eq!(
        cell(&suru, "imperative"),
        ["しろ", "せよ", "するな", "しなさい"]
    );
    assert_eq!(
        cell(&suru, "potential"),
        ["できる", "できない", "できます", "できません"]
    );
    let kanji = conjugate("為る", "する", &["vs"]).unwrap();
    assert_eq!(kanji.term, "する");
    assert_eq!(cell(&kanji, "past")[0], "した");
}

#[test]
fn nouns_taking_suru_get_suru() {
    let table = conjugate("勉強", "べんきょう", &["n", "vs"]).unwrap();
    assert_eq!(table.term, "勉強する");
    assert_eq!(table.reading, "べんきょうする");
    assert_eq!(cell(&table, "past")[0], "勉強した");
    assert_eq!(cell(&table, "potential")[0], "勉強できる");
}

#[test]
fn adjective_forms() {
    let table = conjugate("高い", "たかい", &["adj-i"]).unwrap();
    assert_eq!(
        cell(&table, "past"),
        [
            "高かった",
            "高くなかった",
            "高かったです",
            "高くなかったです",
            "高くありませんでした"
        ]
    );
    assert_eq!(cell(&table, "te"), ["高くて", "高くなくて"]);
    assert_eq!(cell(&table, "noun"), ["高さ"]);

    let ii = conjugate("良い", "いい", &["adj-i"]).unwrap();
    assert_eq!(cell(&ii, "non-past")[0], "良い");
    assert_eq!(cell(&ii, "past")[0], "良かった");
    assert_eq!(readings(&ii, "past")[0], "よかった");
    assert_eq!(
        cell(&conjugate("いい", "", &["adj-i"]).unwrap(), "adverbial")[0],
        "よく"
    );
    assert_eq!(
        cell(&conjugate("かっこいい", "", &["adj-i"]).unwrap(), "past")[0],
        "かっこよかった"
    );

    let kawaii = conjugate("かわいい", "", &["adj-i"]).unwrap();
    assert_eq!(cell(&kawaii, "past")[0], "かわいかった");
    let kanji = conjugate("可愛い", "かわいい", &["adj-i"]).unwrap();
    assert_eq!(cell(&kanji, "non-past")[1], "可愛くない");
    assert_eq!(readings(&kanji, "non-past")[1], "かわいくない");
}

#[test]
fn adjective_class_wins_over_a_suru_tag() {
    let table = conjugate("高い", "たかい", &["vs", "adj-i"]).unwrap();
    assert_eq!(table.word_class, "adj-i");
}

#[test]
fn unsupported_input() {
    assert!(conjugate("猫", "ねこ", &["n"]).is_none());
    assert!(conjugate("猫", "ねこ", &["v5"]).is_none());
    assert!(conjugate("", "", &["v1"]).is_none());
    let long = "あ".repeat(100) + "る";
    assert!(conjugate(&long, "", &["v1"]).is_none());
}

/// True if one of `texts` deinflects to one of `targets`.
fn deinflects(texts: &[String], targets: &[(String, &str)]) -> bool {
    texts.iter().any(|text| {
        transform(text).iter().any(|c| {
            targets.iter().any(|(target, class)| {
                &c.text == target
                    && conditions_match(
                        c.conditions,
                        condition_flags_from_parts_of_speech(&[*class]),
                    )
            })
        })
    })
}

/// Terms of all forms named `name`, in table order.
fn cell(table: &Conjugation, name: &str) -> Vec<String> {
    table
        .forms
        .iter()
        .filter(|f| f.name == name)
        .map(|f| f.term.clone())
        .collect()
}

/// Terms of the forms in one (name, polite, negative) cell.
fn forms(table: &Conjugation, name: &str, polite: bool, negative: bool) -> Vec<String> {
    table
        .forms
        .iter()
        .filter(|f| f.name == name && f.polite == polite && f.negative == negative)
        .map(|f| f.term.clone())
        .collect()
}

/// Readings of all forms named `name`, in table order.
fn readings(table: &Conjugation, name: &str) -> Vec<String> {
    table
        .forms
        .iter()
        .filter(|f| f.name == name)
        .map(|f| f.reading.clone())
        .collect()
}
