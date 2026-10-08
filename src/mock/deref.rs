//! JSON Pointer resolution and reference dereferencing.

use serde_json::Value;

/// Resolves a local JSON pointer `$ref` (e.g. `#/components/schemas/Pet` or `#/definitions/Pet`)
/// against the document root.
pub fn resolve_ref<'a>(root: &'a Value, ref_path: &str) -> Option<&'a Value> {
    let path = ref_path.trim();
    if path == "#" {
        return Some(root);
    }
    if !path.starts_with("#/") {
        return None;
    }

    let pointer = &path[2..];
    let segments = pointer.split('/');

    let mut current = root;
    for raw_seg in segments {
        let seg = unescape_json_pointer(raw_seg);
        match current {
            Value::Object(map) => {
                current = map.get(&seg)?;
            }
            Value::Array(arr) => {
                let idx: usize = seg.parse().ok()?;
                current = arr.get(idx)?;
            }
            _ => return None,
        }
    }

    Some(current)
}

/// Unescapes RFC 6901 JSON Pointer token:
/// `~1` -> `/`, `~0` -> `~`
fn unescape_json_pointer(token: &str) -> String {
    if !token.contains('~') {
        return token.to_string();
    }
    token.replace("~1", "/").replace("~0", "~")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_resolve_ref_swagger_and_openapi() {
        let root = json!({
            "definitions": {
                "User": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "integer"}
                    }
                }
            },
            "components": {
                "schemas": {
                    "Order": {
                        "type": "object",
                        "properties": {
                            "order_id": {"type": "string"}
                        }
                    }
                }
            }
        });

        let user = resolve_ref(&root, "#/definitions/User");
        assert!(user.is_some());
        assert_eq!(user.unwrap()["type"], "object");

        let order = resolve_ref(&root, "#/components/schemas/Order");
        assert!(order.is_some());
        assert_eq!(order.unwrap()["type"], "object");

        let not_found = resolve_ref(&root, "#/components/schemas/NonExistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_unescape_pointer() {
        assert_eq!(unescape_json_pointer("a~1b"), "a/b");
        assert_eq!(unescape_json_pointer("m~0n"), "m~n");
    }
}
