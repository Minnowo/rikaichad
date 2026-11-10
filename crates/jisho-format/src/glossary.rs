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
    }
    Ok(())
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
