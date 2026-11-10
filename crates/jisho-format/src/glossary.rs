// SPDX-License-Identifier: AGPL-3.0-or-later
//! Validation and normalization of term glossary items at import time.

use serde_json::Value;

/// Validates the glossary items of one term and normalizes them the way
/// Yomitan's importer does (`_formatDictionaryTermGlossaryObject`):
/// `{type: "text", text}` becomes the plain string `text`; strings,
/// `structured-content` and `image` objects and deinflection references
/// (`[uninflected, [rules...]]`) are kept as they are.
///
/// Structured content trees are not validated deeply; renderers must
/// tolerate unknown tags and attributes. Their depth is already capped by
/// the JSON parser's recursion limit.
///
/// Returns a message naming the first invalid item.
pub fn normalize_glossary(items: &mut [Value]) -> Result<(), String> {
    for (i, item) in items.iter_mut().enumerate() {
        let replacement = match item {
            Value::String(_) => None,
            Value::Array(parts) => {
                validate_deinflection(parts).map_err(|e| format!("glossary item {i}: {e}"))?;
                None
            }
            Value::Object(map) => match map.get("type").and_then(Value::as_str) {
                Some("text") => match map.get("text") {
                    Some(Value::String(text)) => Some(Value::String(text.clone())),
                    _ => {
                        return Err(format!(
                            "glossary item {i}: text item without \"text\" string"
                        ));
                    }
                },
                Some("structured-content") => {
                    if !map.contains_key("content") {
                        return Err(format!(
                            "glossary item {i}: structured-content item without \"content\""
                        ));
                    }
                    None
                }
                Some("image") => {
                    if !matches!(map.get("path"), Some(Value::String(_))) {
                        return Err(format!("glossary item {i}: image item without \"path\""));
                    }
                    None
                }
                Some(other) => return Err(format!("glossary item {i}: unknown type {other:?}")),
                None => return Err(format!("glossary item {i}: object without \"type\"")),
            },
            _ => {
                return Err(format!(
                    "glossary item {i}: must be a string, object or deinflection array"
                ));
            }
        };
        if let Some(value) = replacement {
            *item = value;
        }
        canonicalize_numbers(item);
    }
    Ok(())
}

/// Writes integral numbers as integers (`1.0` becomes `1`), as JavaScript
/// keeps them: Yomitan compares glossaries by `JSON.stringify`, where the
/// two are the same.
pub fn canonicalize_numbers(value: &mut Value) {
    /// Largest integer a JavaScript number holds exactly.
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
    match value {
        Value::Number(n) if n.is_f64() => {
            if let Some(f) = n.as_f64()
                && f.fract() == 0.0
                && f.abs() <= MAX_SAFE
            {
                // In range, so the conversion is exact.
                #[allow(clippy::cast_possible_truncation)]
                let integer = f as i64;
                *value = Value::from(integer);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(canonicalize_numbers),
        Value::Object(map) => map.values_mut().for_each(canonicalize_numbers),
        _ => {}
    }
}

fn validate_deinflection(parts: &[Value]) -> Result<(), String> {
    match parts {
        [Value::String(_), Value::Array(rules)] if rules.iter().all(Value::is_string) => Ok(()),
        _ => Err("deinflection must be [uninflected term, [rule names]]".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn integral_numbers_become_integers() {
        let mut items = vec![
            json!({"type": "structured-content", "content": {"tag": "td", "colSpan": 1.0, "x": [2.5, -0.0, 3]}}),
        ];
        normalize_glossary(&mut items).unwrap();
        assert_eq!(
            serde_json::to_string(&items).unwrap(),
            r#"[{"content":{"colSpan":1,"tag":"td","x":[2.5,0,3]},"type":"structured-content"}]"#
        );
    }

    #[test]
    fn flattens_text_items_and_keeps_others() {
        let mut items = vec![
            json!("plain"),
            json!({"type": "text", "text": "flattened"}),
            json!({"type": "structured-content", "content": ["a"]}),
            json!({"type": "image", "path": "img/a.png"}),
            json!(["taberu", ["past"]]),
        ];
        normalize_glossary(&mut items).unwrap();
        assert_eq!(items[0], json!("plain"));
        assert_eq!(items[1], json!("flattened"));
        assert_eq!(
            items[2],
            json!({"type": "structured-content", "content": ["a"]})
        );
        assert_eq!(items[4], json!(["taberu", ["past"]]));
    }

    #[test]
    fn rejects_invalid_items() {
        for item in [
            json!(1),
            json!(null),
            json!({"type": "video"}),
            json!({"text": "no type"}),
            json!({"type": "text"}),
            json!({"type": "structured-content"}),
            json!({"type": "image"}),
            json!(["only one"]),
        ] {
            let mut items = vec![json!("ok"), item.clone()];
            let err = normalize_glossary(&mut items).unwrap_err();
            assert!(err.starts_with("glossary item 1"), "{item}: {err}");
        }
    }
}
