// Ported from Yomitan ext/js/language/language-transformer.js at ee81f8dcf13e985bb4d7961009fb49a764b4d924.
// Copyright (C) 2024-2026  Yomitan Authors
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Build script: turns the ported JSON tables in data/ into static Rust
// arrays (OUT_DIR/ja_data.rs), so rikaichad-ja needs no runtime parsing and
// has no failure paths for its data. Condition bit flags are assigned here
// with the algorithm of LanguageTransformer._getConditionFlagsMap: leaf
// conditions get 1 << n in declaration order, conditions with
// subConditions get the OR of their parts. Bad data fails the build.

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

/// Yomitan's limit: flags live in a 32-bit integer.
const MAX_FLAGS: u32 = 32;

fn main() -> Result<(), Box<dyn Error>> {
    let data = Path::new("data");
    for name in [
        "japanese-transforms.json",
        "romaji-to-hiragana.json",
        "nfkd.json",
        "kanji-variants.json",
        "halfwidth-katakana.json",
        "prolonged-sound.json",
    ] {
        println!("cargo::rerun-if-changed=data/{name}");
    }
    let mut out = String::new();
    write_transforms(
        &read_json(&data.join("japanese-transforms.json"))?,
        &mut out,
    )?;
    write_romaji(&read_json(&data.join("romaji-to-hiragana.json"))?, &mut out)?;
    write_nfkd(&read_json(&data.join("nfkd.json"))?, &mut out)?;
    write_kanji_variants(&read_json(&data.join("kanji-variants.json"))?, &mut out)?;
    write_halfwidth(&read_json(&data.join("halfwidth-katakana.json"))?, &mut out)?;
    write_prolonged(&read_json(&data.join("prolonged-sound.json"))?, &mut out)?;
    let out_dir = std::env::var("OUT_DIR")?;
    std::fs::write(Path::new(&out_dir).join("ja_data.rs"), out)?;
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn write_transforms(value: &Value, out: &mut String) -> Result<(), Box<dyn Error>> {
    let conditions = array(value.get("conditions"), "conditions")?;
    let flags = condition_flags(conditions)?;

    out.push_str("/// Conditions: (id, is dictionary form, flags), declaration order.\n");
    out.push_str("pub static CONDITIONS: &[ConditionData] = &[\n");
    for condition in conditions {
        let id = string(condition.get("id"), "condition id")?;
        let dictionary_form = condition
            .get("isDictionaryForm")
            .and_then(Value::as_bool)
            .ok_or("condition isDictionaryForm")?;
        let flag = flags.get(id).ok_or("condition without flags")?;
        writeln!(
            out,
            "    ConditionData {{ id: {id:?}, is_dictionary_form: {dictionary_form}, flags: {flag} }},"
        )?;
    }
    out.push_str("];\n\n");

    out.push_str("/// Transforms in Yomitan's order, rules in rule order.\n");
    out.push_str("pub static TRANSFORMS: &[TransformData] = &[\n");
    for transform in array(value.get("transforms"), "transforms")? {
        let id = string(transform.get("id"), "transform id")?;
        let name = string(transform.get("name"), "transform name")?;
        let description = match transform.get("description") {
            Some(Value::String(d)) => format!("Some({d:?})"),
            _ => "None".to_string(),
        };
        writeln!(
            out,
            "    TransformData {{ id: {id:?}, name: {name:?}, description: {description}, rules: &["
        )?;
        for rule in array(transform.get("rules"), "rules")? {
            let whole_word = match string(rule.get("type"), "rule type")? {
                "suffix" => false,
                "wholeWord" => true,
                other => return Err(format!("unsupported rule type {other}").into()),
            };
            let inflected = string(rule.get("inflected"), "inflected")?;
            let deinflected = string(rule.get("deinflected"), "deinflected")?;
            if inflected.is_empty() {
                return Err(format!("empty inflected text in {id}").into());
            }
            let flags_in = strict_flags(&flags, rule.get("conditionsIn"))?;
            let flags_out = strict_flags(&flags, rule.get("conditionsOut"))?;
            writeln!(
                out,
                "        RuleData {{ whole_word: {whole_word}, inflected: {inflected:?}, deinflected: {deinflected:?}, conditions_in: {flags_in}, conditions_out: {flags_out} }},"
            )?;
        }
        out.push_str("    ] },\n");
    }
    out.push_str("];\n\n");
    Ok(())
}

/// LanguageTransformer._getConditionFlagsMap.
fn condition_flags(conditions: &[Value]) -> Result<HashMap<String, u32>, Box<dyn Error>> {
    let mut map: HashMap<String, u32> = HashMap::new();
    let mut next_flag_index: u32 = 0;
    let mut targets: Vec<&Value> = conditions.iter().collect();
    while !targets.is_empty() {
        let mut next_targets = Vec::new();
        for target in &targets {
            let id = string(target.get("id"), "condition id")?;
            let flags = match target.get("subConditions") {
                None | Some(Value::Null) => {
                    if next_flag_index >= MAX_FLAGS {
                        return Err("Maximum number of conditions was exceeded".into());
                    }
                    let flag = 1u32 << next_flag_index;
                    next_flag_index += 1;
                    flag
                }
                Some(Value::Array(subs)) => {
                    let mut combined = 0u32;
                    let mut missing = false;
                    for sub in subs {
                        match sub.as_str().and_then(|s| map.get(s)) {
                            Some(flag) => combined |= flag,
                            None => missing = true,
                        }
                    }
                    if missing {
                        next_targets.push(*target);
                        continue;
                    }
                    combined
                }
                Some(_) => return Err("subConditions must be an array".into()),
            };
            map.insert(id.to_string(), flags);
        }
        if next_targets.len() == targets.len() {
            return Err("cycle in subConditions".into());
        }
        targets = next_targets;
    }
    Ok(map)
}

/// LanguageTransformer._getConditionFlagsStrict: unknown names are errors.
fn strict_flags(map: &HashMap<String, u32>, value: Option<&Value>) -> Result<u32, Box<dyn Error>> {
    let mut flags = 0u32;
    for name in array(value, "rule conditions")? {
        let name = name.as_str().ok_or("condition name must be a string")?;
        flags |= map
            .get(name)
            .ok_or_else(|| format!("unknown condition {name}"))?;
    }
    Ok(flags)
}

fn write_romaji(value: &Value, out: &mut String) -> Result<(), Box<dyn Error>> {
    out.push_str("/// ROMAJI_TO_HIRAGANA in Object.entries order.\n");
    out.push_str("pub static ROMAJI_TO_HIRAGANA: &[(&str, &str)] = &[\n");
    for pair in array(Some(value), "romaji table")? {
        let romaji = string(pair.get(0), "romaji")?;
        let kana = string(pair.get(1), "kana")?;
        writeln!(out, "    ({romaji:?}, {kana:?}),")?;
    }
    out.push_str("];\n\n");
    Ok(())
}

fn write_nfkd(value: &Value, out: &mut String) -> Result<(), Box<dyn Error>> {
    let mut entries: Vec<(u64, String)> = Vec::new();
    for pair in array(value.get("map"), "nfkd map")? {
        let code_point = pair
            .get(0)
            .and_then(Value::as_u64)
            .ok_or("nfkd code point")?;
        let text = string(pair.get(1), "nfkd text")?;
        entries.push((code_point, text.to_string()));
    }
    entries.sort();
    out.push_str("/// NFKD decompositions (BMP code point, result), sorted by code point.\n");
    out.push_str("pub static NFKD: &[(u32, &str)] = &[\n");
    for (code_point, text) in entries {
        writeln!(out, "    ({code_point:#06x}, {text:?}),")?;
    }
    out.push_str("];\n\n");
    Ok(())
}

fn write_kanji_variants(value: &Value, out: &mut String) -> Result<(), Box<dyn Error>> {
    let mut pairs: Vec<(char, char)> = Vec::new();
    for pair in array(Some(value), "kanji variants")? {
        let variant = single_char(string(pair.get(0), "variant")?)?;
        let standard = single_char(string(pair.get(1), "standard")?)?;
        pairs.push((variant, standard));
    }
    pairs.sort();
    pairs.dedup_by_key(|p| p.0);
    out.push_str("/// Kanji variant -> standard form, sorted by variant.\n");
    out.push_str("pub static KANJI_VARIANTS: &[(char, char)] = &[\n");
    for (variant, standard) in pairs {
        writeln!(out, "    ({variant:?}, {standard:?}),")?;
    }
    out.push_str("];\n");
    Ok(())
}

fn write_halfwidth(value: &Value, out: &mut String) -> Result<(), Box<dyn Error>> {
    let mut rows: Vec<(char, char, Option<char>, Option<char>)> = Vec::new();
    for row in array(Some(value), "halfwidth table")? {
        let optional = |v: Option<&Value>| -> Result<Option<char>, Box<dyn Error>> {
            match v {
                Some(Value::Null) | None => Ok(None),
                Some(Value::String(s)) => Ok(Some(single_char(s)?)),
                Some(_) => Err("halfwidth form must be a string or null".into()),
            }
        };
        rows.push((
            single_char(string(row.get(0), "halfwidth")?)?,
            single_char(string(row.get(1), "fullwidth")?)?,
            optional(row.get(2))?,
            optional(row.get(3))?,
        ));
    }
    rows.sort();
    out.push_str(
        "\n/// Halfwidth katakana: (halfwidth, plain, with dakuten, with handakuten), sorted.\n",
    );
    out.push_str(
        "pub static HALFWIDTH_KATAKANA: &[(char, char, Option<char>, Option<char>)] = &[\n",
    );
    for (half, plain, dakuten, handakuten) in rows {
        writeln!(
            out,
            "    ({half:?}, {plain:?}, {dakuten:?}, {handakuten:?}),"
        )?;
    }
    out.push_str("];\n");
    Ok(())
}

fn write_prolonged(value: &Value, out: &mut String) -> Result<(), Box<dyn Error>> {
    let mut rows: Vec<(char, char)> = Vec::new();
    for row in array(Some(value), "prolonged sound table")? {
        rows.push((
            single_char(string(row.get(0), "kana")?)?,
            single_char(string(row.get(1), "vowel")?)?,
        ));
    }
    rows.sort();
    out.push_str(
        "\n/// Kana -> hiragana vowel that a following long-vowel mark becomes, sorted.\n",
    );
    out.push_str("pub static PROLONGED_SOUND: &[(char, char)] = &[\n");
    for (kana, vowel) in rows {
        writeln!(out, "    ({kana:?}, {vowel:?}),")?;
    }
    out.push_str("];\n");
    Ok(())
}

fn single_char(text: &str) -> Result<char, Box<dyn Error>> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(c),
        _ => Err(format!("expected one character, got {text:?}").into()),
    }
}

fn array<'a>(value: Option<&'a Value>, what: &str) -> Result<&'a [Value], Box<dyn Error>> {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{what} must be an array").into())
}

fn string<'a>(value: Option<&'a Value>, what: &str) -> Result<&'a str, Box<dyn Error>> {
    value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{what} must be a string").into())
}
