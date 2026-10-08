//! Swagger 2.0 parser.

use super::model::{
    NormalizedOperation, NormalizedParameter, NormalizedRequestBody, NormalizedSecurityRequirement,
    NormalizedSpec, ParameterLocation, SpecVersion,
};
use serde_json::Value;

/// Parses a Swagger 2.0 document into a NormalizedSpec.
pub fn parse_swagger2(root: &Value) -> Result<NormalizedSpec, String> {
    let swagger_ver = root
        .get("swagger")
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing 'swagger' field".to_string())?;

    if swagger_ver != "2.0" {
        return Err(format!("Unsupported Swagger version: {swagger_ver}"));
    }

    let info = root.get("info");
    let title = info
        .and_then(|i| i.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Swagger API")
        .to_string();
    let description = info
        .and_then(|i| i.get("description"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    // Compute servers / base URL
    let mut servers = Vec::new();
    let host = root.get("host").and_then(Value::as_str);
    let base_path = root.get("basePath").and_then(Value::as_str).unwrap_or("");
    let schemes = root.get("schemes").and_then(Value::as_array);

    if let Some(h) = host {
        if let Some(s_list) = schemes {
            for s in s_list {
                if let Some(scheme) = s.as_str() {
                    servers.push(format!("{scheme}://{h}{base_path}"));
                }
            }
        }
        if servers.is_empty() {
            servers.push(format!("https://{h}{base_path}"));
        }
    } else if !base_path.is_empty() {
        servers.push(base_path.to_string());
    }

    // Security definitions map: name -> SecurityScheme
    let sec_defs = root.get("securityDefinitions").and_then(Value::as_object);

    let mut operations = Vec::new();
    let paths = root.get("paths").and_then(Value::as_object);

    if let Some(paths_map) = paths {
        for (path_str, path_item) in paths_map {
            let path_common_params = path_item
                .get("parameters")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            let http_methods = ["get", "post", "put", "delete", "patch", "head", "options"];
            for method in &http_methods {
                if let Some(op_val) = path_item.get(*method) {
                    if !op_val.is_object() {
                        continue;
                    }
                    let op = parse_operation(
                        root,
                        path_str,
                        &method.to_ascii_uppercase(),
                        op_val,
                        &path_common_params,
                        sec_defs,
                    )?;
                    operations.push(op);
                }
            }
        }
    }

    Ok(NormalizedSpec {
        version: SpecVersion::Swagger20,
        title,
        description,
        servers,
        operations,
        raw: root.clone(),
    })
}

fn parse_operation(
    root: &Value,
    path: &str,
    method: &str,
    op: &Value,
    path_common_params: &[Value],
    sec_defs: Option<&serde_json::Map<String, Value>>,
) -> Result<NormalizedOperation, String> {
    let operation_id = op
        .get("operationId")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let summary = op
        .get("summary")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let description = op
        .get("description")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let deprecated = op
        .get("deprecated")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let tags = op
        .get("tags")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    // ID format: /users/{id}:get
    let id = format!("{}:{}", path, method.to_ascii_lowercase());

    // Parameters: combine path-level and operation-level
    let mut parameters = Vec::new();
    let mut request_body: Option<NormalizedRequestBody> = None;

    let mut all_param_vals = path_common_params.to_vec();
    if let Some(op_params) = op.get("parameters").and_then(Value::as_array) {
        all_param_vals.extend(op_params.clone());
    }

    for p_val in all_param_vals {
        let resolved_p = if let Some(ref_str) = p_val.get("$ref").and_then(Value::as_str) {
            crate::mock::deref::resolve_ref(root, ref_str)
                .cloned()
                .unwrap_or(p_val)
        } else {
            p_val
        };

        let in_str = resolved_p
            .get("in")
            .and_then(Value::as_str)
            .unwrap_or("query");
        let p_name = resolved_p
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let required = resolved_p
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let p_desc = resolved_p
            .get("description")
            .and_then(Value::as_str)
            .map(|s| s.to_string());

        if in_str == "body" {
            let schema = resolved_p.get("schema").cloned();
            let consumes = op.get("consumes").and_then(Value::as_array);
            let content_type = consumes
                .and_then(|c| c.first())
                .and_then(Value::as_str)
                .unwrap_or("application/json")
                .to_string();

            request_body = Some(NormalizedRequestBody {
                content_type,
                schema,
                example: resolved_p.get("example").cloned(),
                required,
            });
        } else if in_str == "formData" {
            // FormData body
            if request_body.is_none() {
                request_body = Some(NormalizedRequestBody {
                    content_type: "application/x-www-form-urlencoded".to_string(),
                    schema: Some(serde_json::json!({
                        "type": "object",
                        "properties": {}
                    })),
                    example: None,
                    required: false,
                });
            }
            if let Some(rb) = &mut request_body {
                if let Some(s) = &mut rb.schema {
                    if let Some(props) = s.get_mut("properties").and_then(Value::as_object_mut) {
                        let prop_type = resolved_p
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("string");
                        props.insert(
                            p_name.clone(),
                            serde_json::json!({
                                "type": prop_type,
                                "description": p_desc
                            }),
                        );
                    }
                }
            }
            parameters.push(NormalizedParameter {
                name: p_name,
                location: ParameterLocation::Query,
                required,
                description: p_desc,
                schema: Some(resolved_p.clone()),
            });
        } else {
            parameters.push(NormalizedParameter {
                name: p_name,
                location: ParameterLocation::parse_location(in_str),
                required,
                description: p_desc,
                schema: Some(resolved_p.clone()),
            });
        }
    }

    // Security requirements
    let mut security = Vec::new();
    let sec_reqs = op
        .get("security")
        .or_else(|| root.get("security"))
        .and_then(Value::as_array);
    if let (Some(reqs), Some(defs)) = (sec_reqs, sec_defs) {
        for item in reqs {
            if let Some(obj) = item.as_object() {
                for (scheme_name, scopes_val) in obj {
                    if let Some(def) = defs.get(scheme_name) {
                        let sec_type = def.get("type").and_then(Value::as_str).unwrap_or("apiKey");
                        let sec_in = def.get("in").and_then(Value::as_str).map(|s| s.to_string());
                        let sec_scheme = if sec_type == "basic" {
                            Some("basic".to_string())
                        } else {
                            None
                        };
                        let scopes = scopes_val
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(Value::as_str)
                                    .map(|s| s.to_string())
                                    .collect()
                            })
                            .unwrap_or_default();

                        security.push(NormalizedSecurityRequirement {
                            scheme_name: scheme_name.clone(),
                            scheme_type: sec_type.to_string(),
                            scheme_in: sec_in,
                            scheme_scheme: sec_scheme,
                            scopes,
                        });
                    }
                }
            }
        }
    }

    Ok(NormalizedOperation {
        id,
        path: path.to_string(),
        method: method.to_string(),
        operation_id,
        summary,
        description,
        tags,
        deprecated,
        parameters,
        request_body,
        security,
    })
}
