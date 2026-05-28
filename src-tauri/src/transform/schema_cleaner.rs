use serde_json::Value;

/// 递归剥除上游 JSON schema 里不被严格上游识别的字段：`strict`、
/// `additionalProperties`（任意值）、null-valued properties。同时递归进
/// `properties` / `items` / `anyOf` / `oneOf` / `allOf` / `$defs` / `definitions`
/// 子树。
///
/// 用于以下场景：
/// - **DeepSeek / 严格 OpenAI 兼容上游**：会拒识别 `strict:true` 等 OpenAI 字段
/// - **Anthropic**：通常忽略未知字段，但少数情况会 400；过一遍 cleaner 更稳
///
/// 函数无 provider 特化逻辑，名字带 "deepseek" 是历史原因——调用方按场景
/// 自行决定是否调用即可。
pub fn clean_schema_for_deepseek(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("strict");

            // Remove additionalProperties entirely (DeepSeek doesn't support it, regardless of value)
            map.remove("additionalProperties");

            // Clean null-valued properties
            if let Some(Value::Object(props)) = map.get_mut("properties") {
                props.retain(|_, v| !v.is_null());
                for (_, v) in props.iter_mut() {
                    clean_schema_for_deepseek(v);
                }
            }

            // Recurse into items
            if let Some(items) = map.get_mut("items") {
                clean_schema_for_deepseek(items);
            }

            // Recurse into anyOf/oneOf/allOf
            for key in &["anyOf", "oneOf", "allOf"] {
                if let Some(Value::Array(arr)) = map.get_mut(*key) {
                    for item in arr.iter_mut() {
                        clean_schema_for_deepseek(item);
                    }
                }
            }

            // Recurse into $defs/definitions
            for key in &["$defs", "definitions"] {
                if let Some(Value::Object(defs)) = map.get_mut(*key) {
                    for (_, v) in defs.iter_mut() {
                        clean_schema_for_deepseek(v);
                    }
                }
            }

            // additionalProperties is always removed above
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                clean_schema_for_deepseek(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_removes_strict() {
        let mut schema = json!({"type": "object", "strict": true});
        clean_schema_for_deepseek(&mut schema);
        assert!(schema.get("strict").is_none());
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_removes_additional_properties_false() {
        let mut schema = json!({"type": "object", "additionalProperties": false});
        clean_schema_for_deepseek(&mut schema);
        assert!(schema.get("additionalProperties").is_none());
    }

    #[test]
    fn test_removes_additional_properties_true() {
        let mut schema = json!({"type": "object", "additionalProperties": true});
        clean_schema_for_deepseek(&mut schema);
        assert!(schema.get("additionalProperties").is_none());
    }

    #[test]
    fn test_removes_additional_properties_object() {
        let mut schema = json!({"type": "object", "additionalProperties": {"type": "string", "strict": true}});
        clean_schema_for_deepseek(&mut schema);
        assert!(schema.get("additionalProperties").is_none());
    }

    #[test]
    fn test_removes_null_properties() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": null,
                "email": {"type": "string"}
            }
        });
        clean_schema_for_deepseek(&mut schema);
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("name"));
        assert!(!props.contains_key("age"));
        assert!(props.contains_key("email"));
    }

    #[test]
    fn test_recurses_into_items() {
        let mut schema = json!({
            "type": "array",
            "items": {"type": "object", "strict": true, "additionalProperties": false}
        });
        clean_schema_for_deepseek(&mut schema);
        let items = &schema["items"];
        assert!(items.get("strict").is_none());
        assert!(items.get("additionalProperties").is_none());
    }

    #[test]
    fn test_recurses_into_anyof_oneof_allof() {
        let mut schema = json!({
            "anyOf": [
                {"type": "object", "strict": true},
                {"type": "string"}
            ],
            "oneOf": [
                {"type": "number", "additionalProperties": false}
            ],
            "allOf": [
                {"type": "array", "strict": true}
            ]
        });
        clean_schema_for_deepseek(&mut schema);
        assert!(schema["anyOf"][0].get("strict").is_none());
        assert!(schema["oneOf"][0].get("additionalProperties").is_none());
        assert!(schema["allOf"][0].get("strict").is_none());
    }

    #[test]
    fn test_recurses_into_defs() {
        let mut schema = json!({
            "$defs": {
                "User": {"type": "object", "strict": true}
            },
            "definitions": {
                "Item": {"type": "object", "additionalProperties": false}
            }
        });
        clean_schema_for_deepseek(&mut schema);
        assert!(schema["$defs"]["User"].get("strict").is_none());
        assert!(schema["definitions"]["Item"].get("additionalProperties").is_none());
    }

    #[test]
    fn test_nested_complex_schema() {
        let mut schema = json!({
            "type": "object",
            "strict": true,
            "properties": {
                "users": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "strict": true,
                        "properties": {
                            "tags": {
                                "type": "array",
                                "items": {"type": "string", "strict": true}
                            }
                        }
                    }
                }
            }
        });
        clean_schema_for_deepseek(&mut schema);
        assert!(schema.get("strict").is_none());
        assert!(schema["properties"]["users"]["items"].get("strict").is_none());
        assert!(schema["properties"]["users"]["items"]["properties"]["tags"]["items"].get("strict").is_none());
    }
}
