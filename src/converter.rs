//! Converts normalized OpenAPI operations into host DataItem and NodeTemplate structures.

use std::collections::HashMap;

use mpe_plugin_sdk::protocol::datasource::{
    DataItem, DataItemProtocol, KeyValue, NodeTemplate, SourceRef,
};
use serde_json::{json, Map, Value};

use crate::mock::generator::generate_mock;
use crate::parser::model::{
    NormalizedOperation, NormalizedParameter, NormalizedSecurityRequirement, NormalizedSpec,
    ParameterLocation,
};

/// Converts a normalized operation into a browsable DataItem for the host sidebar.
pub fn operation_to_data_item(op: &NormalizedOperation, spec: &NormalizedSpec) -> DataItem {
    let name = op
        .summary
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| op.operation_id.as_deref().filter(|s| !s.trim().is_empty()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{} {}", op.method, op.path));

    let primary_tag = op
        .tags
        .first()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .unwrap_or("Default")
        .to_string();

    let mut extra = vec![
        KeyValue {
            label: crate::i18n::I18n::extra_group().to_string(),
            value: primary_tag,
        },
        KeyValue {
            label: crate::i18n::I18n::extra_spec_version().to_string(),
            value: spec.version.to_string(),
        },
    ];

    if let Some(op_id) = &op.operation_id {
        extra.push(KeyValue {
            label: "OperationId".to_string(),
            value: op_id.clone(),
        });
    }

    if op.deprecated {
        extra.push(KeyValue {
            label: crate::i18n::I18n::extra_deprecated().to_string(),
            value: crate::i18n::I18n::yes().to_string(),
        });
    }

    let raw_ctx = json!({
        "path": op.path,
        "method": op.method,
        "operation_id": op.operation_id,
        "summary": op.summary,
        "description": op.description,
        "tags": op.tags,
        "deprecated": op.deprecated,
        "parameters": op.parameters,
        "request_body": op.request_body,
        "security": op.security,
    });

    DataItem {
        id: op.id.clone(),
        name,
        protocol: DataItemProtocol::Http,
        method: Some(op.method.clone()),
        path: Some(op.path.clone()),
        summary: op.description.clone().or_else(|| op.summary.clone()),
        extra,
        raw: Some(raw_ctx),
    }
}

/// Converts a normalized operation into an MPE NodeTemplate ("http:request").
pub fn operation_to_node_template(
    op: &NormalizedOperation,
    spec: &NormalizedSpec,
    datasource_id: &str,
    base_url_override: Option<&str>,
) -> NodeTemplate {
    // 1. Determine base URL
    let base_url = base_url_override
        .filter(|s| !s.trim().is_empty())
        .or_else(|| spec.servers.first().map(|s| s.as_str()))
        .unwrap_or("")
        .trim_end_matches('/');

    // 2. Interpolate path parameters: {param} -> {{param}}
    let interpolated_path = interpolate_path_params(&op.path);

    // 3. Assemble full URL
    let mut full_url = if base_url.is_empty() {
        interpolated_path
    } else {
        let path_with_slash = if interpolated_path.starts_with('/') {
            interpolated_path
        } else {
            format!("/{interpolated_path}")
        };
        format!("{base_url}{path_with_slash}")
    };

    // 4. Query parameters
    let mut query_params = Vec::new();
    for p in &op.parameters {
        if p.location == ParameterLocation::Query {
            let val = default_param_value(p);
            query_params.push((p.name.clone(), val));
        }
    }

    if !query_params.is_empty() {
        let mut query_parts = Vec::new();
        for (k, v) in query_params {
            if v.is_empty() {
                query_parts.push(k);
            } else {
                query_parts.push(format!("{k}={v}"));
            }
        }
        let sep = if full_url.contains('?') { "&" } else { "?" };
        full_url.push_str(sep);
        full_url.push_str(&query_parts.join("&"));
    }

    // 5. Header parameters
    let mut headers = HashMap::new();
    for p in &op.parameters {
        if p.location == ParameterLocation::Header {
            let val = default_param_value(p);
            headers.insert(p.name.clone(), val);
        }
    }

    // 6. Body & Content-Type
    let mut body_type = "none".to_string();
    let mut content_type = "json".to_string();
    let mut body_str: Option<String> = None;
    let mut form_fields: Option<Vec<Value>> = None;

    if let Some(rb) = &op.request_body {
        let ct_lower = rb.content_type.to_ascii_lowercase();
        headers.insert("Content-Type".to_string(), rb.content_type.clone());

        if ct_lower.contains("json") {
            body_type = "body".to_string();
            content_type = "json".to_string();

            if let Some(example_val) = &rb.example {
                body_str = serde_json::to_string_pretty(example_val).ok();
            } else if let Some(schema) = &rb.schema {
                let mock_val = generate_mock(schema, &spec.raw);
                body_str = serde_json::to_string_pretty(&mock_val).ok();
            }
        } else if ct_lower.contains("x-www-form-urlencoded") {
            body_type = "x-www-form-urlencoded".to_string();
            content_type = "text".to_string();

            let mut fields = Vec::new();
            if let Some(schema) = &rb.schema {
                if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                    for (prop_name, prop_schema) in props {
                        let mock_val = generate_mock(prop_schema, &spec.raw);
                        let val_str = match mock_val {
                            Value::String(s) => s,
                            Value::Null => String::new(),
                            other => other.to_string(),
                        };
                        fields.push(json!({
                            "key": prop_name,
                            "value": val_str,
                            "field_type": "text"
                        }));
                    }
                }
            }
            form_fields = Some(fields);
        } else if ct_lower.contains("form-data") {
            body_type = "form-data".to_string();
            content_type = "text".to_string();

            let mut fields = Vec::new();
            if let Some(schema) = &rb.schema {
                if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                    for (prop_name, prop_schema) in props {
                        let is_file = prop_schema
                            .get("format")
                            .and_then(Value::as_str)
                            .map(|f| f == "binary" || f == "byte")
                            .unwrap_or(false)
                            || prop_schema
                                .get("type")
                                .and_then(Value::as_str)
                                .map(|t| t == "file")
                                .unwrap_or(false);

                        fields.push(json!({
                            "key": prop_name,
                            "value": if is_file { "" } else { "text_val" },
                            "field_type": if is_file { "file" } else { "text" }
                        }));
                    }
                }
            }
            form_fields = Some(fields);
        } else {
            body_type = "body".to_string();
            content_type = "text".to_string();
            if let Some(schema) = &rb.schema {
                let mock_val = generate_mock(schema, &spec.raw);
                body_str = Some(mock_val.to_string());
            }
        }
    }

    // 7. Security & Authentication mapping
    let auth = map_security_to_auth(&op.security, &mut headers, &mut full_url);

    // 8. Build NodeTemplate data
    let mut node_data = Map::new();
    node_data.insert("type".to_string(), json!("http:request"));
    node_data.insert("url".to_string(), json!(full_url));
    node_data.insert("method".to_string(), json!(op.method.to_ascii_lowercase()));
    if !headers.is_empty() {
        node_data.insert("headers".to_string(), json!(headers));
    }
    node_data.insert("body_type".to_string(), json!(body_type));
    node_data.insert("content_type".to_string(), json!(content_type));
    if let Some(b) = body_str {
        node_data.insert("body".to_string(), json!(b));
    }
    if let Some(ff) = form_fields {
        node_data.insert("form_fields".to_string(), json!(ff));
    }
    if let Some(a) = auth {
        node_data.insert("auth".to_string(), a);
    }

    let template_name = op
        .summary
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| op.operation_id.as_deref().filter(|s| !s.trim().is_empty()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{} {}", op.method, op.path));

    NodeTemplate {
        type_id: "http:request".to_string(),
        name: template_name,
        data: Value::Object(node_data),
        source_ref: Some(SourceRef {
            datasource: datasource_id.to_string(),
            item_id: op.id.clone(),
            updated_at: None,
        }),
    }
}

/// Interpolates Swagger / OpenAPI path params `{id}` into MPE variable notation `{{id}}`.
pub fn interpolate_path_params(path: &str) -> String {
    let mut result = String::with_capacity(path.len() + 8);
    let mut in_brace = false;
    let mut param = String::new();

    for ch in path.chars() {
        match ch {
            '{' => {
                in_brace = true;
                param.clear();
            }
            '}' if in_brace => {
                in_brace = false;
                result.push_str("{{");
                result.push_str(&param);
                result.push_str("}}");
            }
            _ => {
                if in_brace {
                    param.push(ch);
                } else {
                    result.push(ch);
                }
            }
        }
    }

    result
}

fn default_param_value(p: &NormalizedParameter) -> String {
    if let Some(schema) = &p.schema {
        if let Some(def) = schema.get("default") {
            match def {
                Value::String(s) => return s.clone(),
                other => return other.to_string(),
            }
        }
        if let Some(ex) = schema.get("example") {
            match ex {
                Value::String(s) => return s.clone(),
                other => return other.to_string(),
            }
        }
    }
    if p.required {
        format!("{{{{{}}}}}", p.name)
    } else {
        String::new()
    }
}

fn map_security_to_auth(
    sec_reqs: &[NormalizedSecurityRequirement],
    headers: &mut HashMap<String, String>,
    full_url: &mut String,
) -> Option<Value> {
    for sec in sec_reqs {
        let scheme_type = sec.scheme_type.to_ascii_lowercase();
        let scheme_scheme = sec
            .scheme_scheme
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();

        if scheme_type == "http" && scheme_scheme == "bearer" {
            return Some(json!({
                "auth_type": "bearer",
                "token": "{{token}}"
            }));
        }

        if scheme_type == "http" && scheme_scheme == "basic" {
            return Some(json!({
                "auth_type": "basic",
                "username": "{{username}}",
                "password": "{{password}}"
            }));
        }

        if scheme_type == "apikey" {
            let loc = sec
                .scheme_in
                .as_deref()
                .unwrap_or("header")
                .to_ascii_lowercase();
            let key_name = &sec.scheme_name;
            if loc == "header" {
                headers.insert(key_name.clone(), format!("{{{{{key_name}}}}}"));
            } else if loc == "query" {
                let sep = if full_url.contains('?') { "&" } else { "?" };
                full_url.push_str(&format!("{sep}{key_name}={{{{{key_name}}}}}"));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_path_params() {
        assert_eq!(interpolate_path_params("/users/{id}"), "/users/{{id}}");
        assert_eq!(
            interpolate_path_params("/users/{user_id}/orders/{order_id}"),
            "/users/{{user_id}}/orders/{{order_id}}"
        );
        assert_eq!(interpolate_path_params("/static/path"), "/static/path");
    }

    #[test]
    fn test_operation_to_template_conversion() {
        let op = NormalizedOperation {
            id: "/users/{id}:get".to_string(),
            path: "/users/{id}".to_string(),
            method: "GET".to_string(),
            operation_id: Some("getUserById".to_string()),
            summary: Some("Get User".to_string()),
            description: Some("Retrieve user info".to_string()),
            tags: vec!["User Management".to_string()],
            deprecated: false,
            parameters: vec![
                NormalizedParameter {
                    name: "id".to_string(),
                    location: ParameterLocation::Path,
                    required: true,
                    description: None,
                    schema: Some(json!({"type": "integer"})),
                },
                NormalizedParameter {
                    name: "detailed".to_string(),
                    location: ParameterLocation::Query,
                    required: false,
                    description: None,
                    schema: Some(json!({"type": "boolean", "default": true})),
                },
            ],
            request_body: None,
            security: vec![NormalizedSecurityRequirement {
                scheme_name: "bearerAuth".to_string(),
                scheme_type: "http".to_string(),
                scheme_in: None,
                scheme_scheme: Some("bearer".to_string()),
                scopes: vec![],
            }],
        };

        let spec = NormalizedSpec {
            version: crate::parser::model::SpecVersion::Swagger20,
            title: "Test API".to_string(),
            description: None,
            servers: vec!["https://api.example.com".to_string()],
            operations: vec![op.clone()],
            raw: json!({}),
        };

        let item = operation_to_data_item(&op, &spec);
        assert_eq!(item.name, "Get User");
        assert_eq!(item.protocol, DataItemProtocol::Http);
        assert!(item.extra[0].label == "分组" || item.extra[0].label == "Group");
        assert_eq!(item.extra[0].value, "User Management");

        let tpl = operation_to_node_template(&op, &spec, "openapi", None);
        assert_eq!(tpl.type_id, "http:request");
        assert_eq!(tpl.name, "Get User");
        let data = &tpl.data;
        assert_eq!(data["method"], "get");
        let url = data["url"].as_str().unwrap();
        assert!(url.starts_with("https://api.example.com/users/{{id}}"));
        assert!(url.contains("detailed=true"));
        assert_eq!(data["auth"]["auth_type"], "bearer");
    }
}
