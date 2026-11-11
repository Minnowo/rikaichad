// SPDX-License-Identifier: AGPL-3.0-or-later
//! Plain-text rendering of glossary items for the CLI and the API's
//! `glossary_text` field.
//!
//! Yomitan renders structured content to HTML and takes the browser's
//! `innerText`, which cannot be reproduced without a DOM. These rules are
//! rikaichad's own (docs/api.md): block tags start new lines, list items become
//! "- " lines ("1. ", "2. ", ... in ordered lists), table cells are joined with " | ", ruby becomes
//! `base(reading)` unless the dictionary supplies its own `rp` parentheses,
//! images become their alt/title/description text or "[image]". Clients
//! that need faithful rendering use the raw glossary JSON.

use serde_json::{Map, Value};

const BLOCK_TAGS: [&str; 10] = [
    "div", "ul", "ol", "li", "table", "thead", "tbody", "tfoot", "tr", "details",
];

/// Renders one glossary item (as stored, see `normalize_glossary`) to plain
/// text. Never fails; unknown shapes render their text content or nothing.
pub fn glossary_item_text(item: &Value) -> String {
    match item {
        Value::String(text) => text.clone(),
        Value::Array(parts) => deinflection_text(parts),
        Value::Object(map) => match map.get("type").and_then(Value::as_str) {
            Some("structured-content") => {
                let mut out = String::new();
                if let Some(content) = map.get("content") {
                    render(content, &mut out);
                }
                tidy(&out)
            }
            Some("image") => image_text(map),
            Some("text") => map
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn render(node: &Value, out: &mut String) {
    match node {
        Value::String(text) => out.push_str(text),
        Value::Array(children) => {
            for child in children {
                render(child, out);
            }
        }
        Value::Object(map) => render_element(map, out),
        _ => {}
    }
}

fn render_element(map: &Map<String, Value>, out: &mut String) {
    let tag = map.get("tag").and_then(Value::as_str).unwrap_or_default();
    let content = map.get("content");
    match tag {
        "br" => out.push('\n'),
        "img" => out.push_str(&image_text(map)),
        "ruby" => render_ruby(content, out),
        "ol" => render_ordered_list(content, out),
        "li" => {
            start_line(out);
            out.push_str("- ");
            render_opt(content, out);
            start_line(out);
        }
        "td" | "th" => {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(" | ");
            }
            render_opt(content, out);
        }
        "summary" => {
            start_line(out);
            render_opt(content, out);
            start_line(out);
        }
        _ if BLOCK_TAGS.contains(&tag) => {
            start_line(out);
            render_opt(content, out);
            start_line(out);
        }
        _ => render_opt(content, out),
    }
}

/// An ordered list: its `li` children numbered from 1; anything else in
/// it rendered as usual.
fn render_ordered_list(content: Option<&Value>, out: &mut String) {
    start_line(out);
    let children: Vec<&Value> = match content {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(other) => vec![other],
        None => Vec::new(),
    };
    let mut number = 0;
    for child in children {
        match child {
            Value::Object(map) if tag_of(child) == Some("li") => {
                number += 1;
                start_line(out);
                out.push_str(&format!("{number}. "));
                render_opt(map.get("content"), out);
                start_line(out);
            }
            other => render(other, out),
        }
    }
    start_line(out);
}

/// Ruby: `base(reading)`. If the dictionary provides `rp` fallback
/// parentheses, its own layout is kept instead (base + rp + rt + rp).
fn render_ruby(content: Option<&Value>, out: &mut String) {
    let Some(content) = content else {
        return;
    };
    let children: Vec<&Value> = match content {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    if children.iter().any(|c| tag_of(c) == Some("rp")) {
        for child in children {
            render(child, out);
        }
        return;
    }
    let mut base = String::new();
    let mut reading = String::new();
    for child in children {
        if tag_of(child) == Some("rt") {
            if let Value::Object(map) = child {
                render_opt(map.get("content"), &mut reading);
            }
        } else {
            render(child, &mut base);
        }
    }
    out.push_str(&base);
    if !reading.is_empty() {
        out.push('(');
        out.push_str(&reading);
        out.push(')');
    }
}

fn render_opt(content: Option<&Value>, out: &mut String) {
    if let Some(content) = content {
        render(content, out);
    }
}

fn tag_of(node: &Value) -> Option<&str> {
    match node {
        Value::Object(map) => map.get("tag").and_then(Value::as_str),
        _ => None,
    }
}

fn image_text(map: &Map<String, Value>) -> String {
    for key in ["alt", "title", "description"] {
        if let Some(text) = map.get(key).and_then(Value::as_str)
            && !text.is_empty()
        {
            return text.to_string();
        }
    }
    "[image]".to_string()
}

fn deinflection_text(parts: &[Value]) -> String {
    let base = parts.first().and_then(Value::as_str).unwrap_or_default();
    let rules: Vec<&str> = parts
        .get(1)
        .and_then(Value::as_array)
        .map(|rules| rules.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if rules.is_empty() {
        format!("form of {base}")
    } else {
        format!("form of {base} ({})", rules.join(", "))
    }
}

fn start_line(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Trims trailing spaces per line, drops blank lines and outer whitespace.
fn tidy(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sc(content: Value) -> String {
        glossary_item_text(&json!({"type": "structured-content", "content": content}))
    }

    #[test]
    fn plain_items() {
        assert_eq!(glossary_item_text(&json!("to eat")), "to eat");
        assert_eq!(
            glossary_item_text(&json!({"type": "image", "path": "a.png", "alt": "diagram"})),
            "diagram"
        );
        assert_eq!(
            glossary_item_text(&json!({"type": "image", "path": "a.png"})),
            "[image]"
        );
        assert_eq!(
            glossary_item_text(&json!(["taberu", ["past", "negative"]])),
            "form of taberu (past, negative)"
        );
        assert_eq!(glossary_item_text(&json!(5)), "");
    }

    #[test]
    fn lists_blocks_and_breaks() {
        let text = sc(json!([
            {"tag": "div", "content": "heading"},
            {"tag": "ul", "content": [
                {"tag": "li", "content": "one"},
                {"tag": "li", "content": ["two", {"tag": "br"}, "cont."]}
            ]},
            "tail"
        ]));
        assert_eq!(text, "heading\n- one\n- two\ncont.\ntail");
    }

    #[test]
    fn ruby_with_and_without_rp() {
        assert_eq!(
            sc(json!({"tag": "ruby", "content": ["漢字", {"tag": "rt", "content": "かんじ"}]})),
            "漢字(かんじ)"
        );
        assert_eq!(
            sc(json!({"tag": "ruby", "content": [
                "漢", {"tag": "rp", "content": "("}, {"tag": "rt", "content": "かん"},
                {"tag": "rp", "content": ")"}
            ]})),
            "漢(かん)"
        );
    }

    /// Every tag of the structured-content schema
    /// (docs/reference/yomitan-schemas) has a case here, and the cases
    /// match the schema's tag list exactly.
    #[test]
    fn every_schema_tag_renders() {
        let cases: Vec<(&str, Value, &str)> = vec![
            ("br", json!(["a", {"tag": "br"}, "b"]), "a\nb"),
            (
                "ruby",
                json!({"tag": "ruby", "content": ["字", {"tag": "rt", "content": "じ"}]}),
                "字(じ)",
            ),
            ("rt", json!(["x", {"tag": "rt", "content": "y"}]), "xy"),
            (
                "rp",
                json!({"tag": "ruby", "content": ["字", {"tag": "rp", "content": "["}, {"tag": "rt", "content": "じ"}, {"tag": "rp", "content": "]"}]}),
                "字[じ]",
            ),
            (
                "table",
                json!({"tag": "table", "content": {"tag": "tr", "content": [{"tag": "td", "content": "a"}, {"tag": "td", "content": "b"}]}}),
                "a | b",
            ),
            (
                "thead",
                json!({"tag": "table", "content": [
                    {"tag": "thead", "content": {"tag": "tr", "content": [{"tag": "th", "content": "h1"}, {"tag": "th", "content": "h2"}]}},
                    {"tag": "tbody", "content": {"tag": "tr", "content": [{"tag": "td", "content": "a"}, {"tag": "td", "content": "b"}]}},
                    {"tag": "tfoot", "content": {"tag": "tr", "content": {"tag": "td", "content": "f"}}}
                ]}),
                "h1 | h2\na | b\nf",
            ),
            (
                "tbody",
                json!({"tag": "tbody", "content": {"tag": "tr", "content": {"tag": "td", "content": "a"}}}),
                "a",
            ),
            (
                "tfoot",
                json!({"tag": "tfoot", "content": {"tag": "tr", "content": {"tag": "td", "content": "f"}}}),
                "f",
            ),
            (
                "tr",
                json!([{"tag": "tr", "content": {"tag": "td", "content": "1"}}, {"tag": "tr", "content": {"tag": "td", "content": "2"}}]),
                "1\n2",
            ),
            (
                "td",
                json!({"tag": "tr", "content": [{"tag": "td", "content": "a"}, {"tag": "td", "content": "b"}]}),
                "a | b",
            ),
            (
                "th",
                json!({"tag": "tr", "content": [{"tag": "th", "content": "a"}, {"tag": "td", "content": "b"}]}),
                "a | b",
            ),
            (
                "span",
                json!(["a", {"tag": "span", "content": "b"}, "c"]),
                "abc",
            ),
            (
                "div",
                json!(["a", {"tag": "div", "content": "b"}, "c"]),
                "a\nb\nc",
            ),
            (
                "ol",
                json!({"tag": "ol", "content": [{"tag": "li", "content": "one"}, {"tag": "li", "content": "two"}]}),
                "1. one\n2. two",
            ),
            (
                "ul",
                json!({"tag": "ul", "content": [{"tag": "li", "content": "one"}, {"tag": "li", "content": "two"}]}),
                "- one\n- two",
            ),
            ("li", json!({"tag": "li", "content": "x"}), "- x"),
            (
                "details",
                json!({"tag": "details", "content": [{"tag": "summary", "content": "more"}, "hidden"]}),
                "more\nhidden",
            ),
            (
                "summary",
                json!(["a", {"tag": "summary", "content": "s"}, "b"]),
                "a\ns\nb",
            ),
            (
                "img",
                json!({"tag": "img", "path": "a.png", "alt": "pic"}),
                "pic",
            ),
            (
                "a",
                json!({"tag": "a", "href": "?query=x", "content": "x"}),
                "x",
            ),
        ];
        let schema: Value = serde_json::from_str(include_str!(
            "../../../docs/reference/yomitan-schemas/dictionary-term-bank-v3-schema.json"
        ))
        .unwrap();
        let mut tags = Vec::new();
        collect_tags(&schema, &mut tags);
        tags.sort();
        tags.dedup();
        let mut covered: Vec<&str> = cases.iter().map(|(t, _, _)| *t).collect();
        covered.sort();
        assert_eq!(covered, tags.iter().map(String::as_str).collect::<Vec<_>>());
        for (tag, content, expected) in cases {
            assert_eq!(sc(content), expected, "{tag}");
        }
    }

    fn collect_tags(value: &Value, tags: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(tag) = map.get("properties").and_then(|p| p.get("tag")) {
                    if let Some(name) = tag.get("const").and_then(Value::as_str) {
                        tags.push(name.to_string());
                    }
                    for name in tag
                        .get("enum")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        tags.extend(name.as_str().map(str::to_string));
                    }
                }
                map.values().for_each(|v| collect_tags(v, tags));
            }
            Value::Array(items) => items.iter().for_each(|v| collect_tags(v, tags)),
            _ => {}
        }
    }

    #[test]
    fn tables_links_images_and_unknown_tags() {
        let text = sc(json!({"tag": "table", "content": [
            {"tag": "tr", "content": [{"tag": "th", "content": "a"}, {"tag": "td", "content": "b"}]},
            {"tag": "tr", "content": [{"tag": "td", "content": "c"}, {"tag": "td", "content": "d"}]}
        ]}));
        assert_eq!(text, "a | b\nc | d");
        assert_eq!(
            sc(
                json!(["see ", {"tag": "a", "href": "?query=x", "content": "x"}, " ",
                      {"tag": "img", "path": "i.png", "title": "icon"}, {"tag": "blink", "content": "!"}])
            ),
            "see x icon!"
        );
    }
}
