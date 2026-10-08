//! Mock body and value generator from JSON Schemas.

use super::deref::resolve_ref;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

const MAX_DEPTH: usize = 5;

/// Generates a mock value for a given schema against the document root.
pub fn generate_mock(schema: &Value, root: &Value) -> Value {
    let mut visited_refs = HashSet::new();
    generate_mock_value(schema, root, 0, &mut visited_refs)
}

/// Recursively generates a mock value, capping depth at 5 and detecting reference cycles.
pub fn generate_mock_value(
    schema: &Value,
    root: &Value,
    depth: usize,
    visited_refs: &mut HashSet<String>,
) -> Value {
    if depth >= MAX_DEPTH {
        return Value::Null;
    }

    // 1. Resolve $ref if present
    if let Some(ref_path) = schema.get("$ref").and_then(Value::as_str) {
        if visited_refs.contains(ref_path) {
            return Value::Null;
        }
        visited_refs.insert(ref_path.to_string());
        let res = if let Some(target) = resolve_ref(root, ref_path) {
            generate_mock_value(target, root, depth + 1, visited_refs)
        } else {
            Value::Null
        };
        visited_refs.remove(ref_path);
        return res;
    }

    // 2. Highest priority: explicit example or default
    if let Some(ex) = schema.get("example") {
        return ex.clone();
    }
    if let Some(def) = schema.get("default") {
        return def.clone();
    }

    // 3. Enum values
    if let Some(enum_vals) = schema.get("enum").and_then(Value::as_array) {
        if let Some(first) = enum_vals.first() {
            return first.clone();
        }
    }

    // 4. Composition: allOf, oneOf, anyOf
    if let Some(all_of) = schema.get("allOf").and_then(Value::as_array) {
        let mut merged_obj = Map::new();
        for sub_schema in all_of {
            let sub_val = generate_mock_value(sub_schema, root, depth + 1, visited_refs);
            if let Value::Object(sub_map) = sub_val {
                for (k, v) in sub_map {
                    merged_obj.insert(k, v);
                }
            }
        }
        return Value::Object(merged_obj);
    }
    if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
        if let Some(first) = one_of.first() {
            return generate_mock_value(first, root, depth + 1, visited_refs);
        }
    }
    if let Some(any_of) = schema.get("anyOf").and_then(Value::as_array) {
        if let Some(first) = any_of.first() {
            return generate_mock_value(first, root, depth + 1, visited_refs);
        }
    }

    // 5. Schema Type resolution
    let schema_type = schema
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| {
            if schema.get("properties").is_some() {
                Some("object")
            } else if schema.get("items").is_some() {
                Some("array")
            } else {
                None
            }
        })
        .unwrap_or("object");

    match schema_type {
        "string" => generate_mock_string(schema),
        "integer" | "number" => generate_mock_number(schema),
        "boolean" => json!(true),
        "array" => {
            let mut arr = Vec::new();
            if let Some(items_schema) = schema.get("items") {
                let item_val = generate_mock_value(items_schema, root, depth + 1, visited_refs);
                if !item_val.is_null() {
                    arr.push(item_val);
                }
            }
            Value::Array(arr)
        }
        "object" => {
            let mut obj = Map::new();
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                for (prop_name, prop_schema) in props {
                    let prop_val = generate_mock_value(prop_schema, root, depth + 1, visited_refs);
                    obj.insert(prop_name.clone(), prop_val);
                }
            } else if let Some(add_props) = schema.get("additionalProperties") {
                if add_props.is_boolean() && add_props.as_bool() == Some(true) {
                    obj.insert("key".to_string(), json!("value"));
                } else if add_props.is_object() {
                    let val = generate_mock_value(add_props, root, depth + 1, visited_refs);
                    obj.insert("key".to_string(), val);
                }
            }
            Value::Object(obj)
        }
        _ => json!({}),
    }
}

fn generate_mock_string(schema: &Value) -> Value {
    let format = schema.get("format").and_then(Value::as_str).unwrap_or("");
    match format {
        "date-time" => json!("2026-08-05T12:00:00Z"),
        "date" => json!("2026-08-05"),
        "time" => json!("12:00:00Z"),
        "email" => json!("user@example.com"),
        "uuid" => json!("3fa85f64-5717-4562-b3fc-2c963f66afa6"),
        "uri" | "url" => json!("https://example.com"),
        "ipv4" => json!("192.168.1.1"),
        "ipv6" => json!("::1"),
        "hostname" => json!("example.com"),
        "byte" | "binary" => json!("dGVzdA=="),
        "password" => json!("secret123"),
        _ => json!("string"),
    }
}

fn generate_mock_number(schema: &Value) -> Value {
    if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
        return json!(min);
    }
    if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
        if max < 0.0 {
            return json!(max);
        }
    }
    json!(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_depth_limit() {
        // Recursive schema
        let root = json!({
            "components": {
                "schemas": {
                    "Node": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"},
                            "child": {"$ref": "#/components/schemas/Node"}
                        }
                    }
                }
            }
        });

        let schema = json!({"$ref": "#/components/schemas/Node"});
        let result = generate_mock(&schema, &root);
        assert!(result.is_object());
        assert_eq!(result["name"], "string");
        // Circular reference is cut off safely
        assert_eq!(result["child"], Value::Null);
    }

    #[test]
    fn test_mock_formats_and_composition() {
        let root = json!({});
        let schema = json!({
            "type": "object",
            "properties": {
                "created_at": {"type": "string", "format": "date-time"},
                "email": {"type": "string", "format": "email"},
                "active": {"type": "boolean"},
                "count": {"type": "integer", "minimum": 10},
                "tags": {
                    "type": "array",
                    "items": {"type": "string"}
                }
            }
        });

        let mock = generate_mock(&schema, &root);
        assert_eq!(mock["created_at"], "2026-08-05T12:00:00Z");
        assert_eq!(mock["email"], "user@example.com");
        assert_eq!(mock["active"], true);
        assert_eq!(mock["count"], 10.0);
        assert_eq!(mock["tags"], json!(["string"]));
    }
}
