//! OpenAPI 3.0 / 3.1 parser.

use super::model::{
    NormalizedOperation, NormalizedParameter, NormalizedRequestBody, NormalizedSecurityRequirement,
    NormalizedSpec, ParameterLocation, SpecVersion,
};
use serde_json::Value;

/// Parses an OpenAPI 3.0 / 3.1 document into a NormalizedSpec.
/// `doc_base_url` is the optional origin/base URL of the document when fetched from remote.
pub fn parse_openapi3(root: &Value, doc_base_url: Option<&str>) -> Result<NormalizedSpec, String> {
    let openapi_ver = root
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing 'openapi' field".to_string())?;

    if !openapi_ver.starts_with("3.") {
        return Err(format!("Unsupported OpenAPI version: {openapi_ver}"));
    }

    let spec_ver = if openapi_ver.starts_with("3.0") {
        SpecVersion::OpenApi30(openapi_ver.to_string())
    } else {
        SpecVersion::OpenApi31(openapi_ver.to_string())
    };

    let info = root.get("info");
    let title = info
        .and_then(|i| i.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("OpenAPI")
        .to_string();
    let description = info
        .and_then(|i| i.get("description"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    // Servers
    let mut servers = Vec::new();
    if let Some(servers_arr) = root.get("servers").and_then(Value::as_array) {
        for s in servers_arr {
            if let Some(u) = s.get("url").and_then(Value::as_str) {
                let resolved = resolve_server_url(u, doc_base_url);
                if !resolved.is_empty() {
                    servers.push(resolved);
                }
            }
        }
    }
    if servers.is_empty() {
        if let Some(base) = doc_base_url {
            servers.push(base.to_string());
        }
    }

    // Security schemes in components.securitySchemes
    let sec_schemes = root
        .get("components")
        .and_then(|c| c.get("securitySchemes"))
        .and_then(Value::as_object);

    let mut operations = Vec::new();
    let paths = root.get("paths").and_then(Value::as_object);

    if let Some(paths_map) = paths {
        for (path_str, path_item) in paths_map {
            let path_common_params = path_item
                .get("parameters")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            let http_methods = [
                "get", "post", "put", "delete", "patch", "head", "options", "trace",
            ];
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
                        sec_schemes,
                    )?;
                    operations.push(op);
                }
            }
        }
    }

    Ok(NormalizedSpec {
        version: spec_ver,
        title,
        description,
        servers,
        operations,
        raw: root.clone(),
    })
}

fn resolve_server_url(server_url: &str, doc_base_url: Option<&str>) -> String {
    let trimmed = server_url.trim();
    if trimmed.is_empty() {
        return doc_base_url.unwrap_or("").to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    // Relative URL like "/api/v1" or "v1"
    if let Some(doc_url) = doc_base_url {
        if let Ok(base_parsed) = url::Url::parse(doc_url) {
            if let Ok(joined) = base_parsed.join(trimmed) {
                return joined.to_string();
            }
        }
    }
    trimmed.to_string()
}

fn parse_operation(
    root: &Value,
    path: &str,
    method: &str,
    op: &Value,
    path_common_params: &[Value],
    sec_schemes: Option<&serde_json::Map<String, Value>>,
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

    let id = format!("{}:{}", path, method.to_ascii_lowercase());

    // Parameters
    let mut parameters = Vec::new();
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
        let p_schema = resolved_p.get("schema").cloned();

        parameters.push(NormalizedParameter {
            name: p_name,
            location: ParameterLocation::parse_location(in_str),
            required,
            description: p_desc,
            schema: p_schema,
        });
    }

    // Request Body
    let mut request_body: Option<NormalizedRequestBody> = None;
    if let Some(rb_raw) = op.get("requestBody") {
        let rb_val = if let Some(ref_str) = rb_raw.get("$ref").and_then(Value::as_str) {
            crate::mock::deref::resolve_ref(root, ref_str).unwrap_or(rb_raw)
        } else {
            rb_raw
        };

        let required = rb_val
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(content) = rb_val.get("content").and_then(Value::as_object) {
            // Priority selection: application/json -> application/x-www-form-urlencoded -> multipart/form-data -> first
            let preferred_media_types = [
                "application/json",
                "application/x-www-form-urlencoded",
                "multipart/form-data",
            ];

            let mut selected_ct = None;
            for p_mt in &preferred_media_types {
                if let Some(media_obj) = content.get(*p_mt) {
                    selected_ct = Some((p_mt.to_string(), media_obj));
                    break;
                }
            }

            if selected_ct.is_none() {
                if let Some((ct, media_obj)) = content.iter().next() {
                    selected_ct = Some((ct.clone(), media_obj));
                }
            }

            if let Some((ct, media_obj)) = selected_ct {
                let schema = media_obj.get("schema").cloned();
                let example = media_obj.get("example").cloned().or_else(|| {
                    media_obj
                        .get("examples")
                        .and_then(Value::as_object)
                        .and_then(|exs| exs.values().next())
                        .and_then(|ex| ex.get("value"))
                        .cloned()
                });

                request_body = Some(NormalizedRequestBody {
                    content_type: ct,
                    schema,
                    example,
                    required,
                });
            }
        }
    }

    // Security requirements
    let mut security = Vec::new();
    let sec_reqs = op
        .get("security")
        .or_else(|| root.get("security"))
        .and_then(Value::as_array);
    if let (Some(reqs), Some(schemes)) = (sec_reqs, sec_schemes) {
        for item in reqs {
            if let Some(obj) = item.as_object() {
                for (scheme_name, scopes_val) in obj {
                    if let Some(def_val) = schemes.get(scheme_name) {
                        let def = if let Some(ref_str) = def_val.get("$ref").and_then(Value::as_str)
                        {
                            crate::mock::deref::resolve_ref(root, ref_str).unwrap_or(def_val)
                        } else {
                            def_val
                        };

                        let sec_type = def.get("type").and_then(Value::as_str).unwrap_or("http");
                        let sec_in = def.get("in").and_then(Value::as_str).map(|s| s.to_string());
                        let sec_scheme = def
                            .get("scheme")
                            .and_then(Value::as_str)
                            .map(|s| s.to_string());
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
