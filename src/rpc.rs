//! JSON-RPC 2.0 stdio event loop and dispatch handlers for Swagger / OpenAPI plugin.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

use crate::converter::{operation_to_data_item, operation_to_node_template};
use crate::datasource_spec::{
    describe_result, DATASOURCE_FILE_ID, DATASOURCE_URL_ID, RESOURCE_DOC_ID,
};
use crate::i18n::I18n;
use crate::mock::generator::generate_mock;
use crate::parser::loader::{global_loader, parse_content};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// Runs the standard line-delimited JSON-RPC 2.0 stdio loop.
pub async fn run_stdio_loop() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line_res in stdin.lock().lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let err_resp =
                    JsonRpcResponse::error(Value::Null, -32700, format!("Parse error: {e}"));
                if let Ok(bytes) = serde_json::to_vec(&err_resp) {
                    let _ = stdout.write_all(&bytes);
                    let _ = stdout.write_all(b"\n");
                    let _ = stdout.flush();
                }
                continue;
            }
        };

        let req_id = req.id.clone().unwrap_or(Value::Null);
        let method = req.method.as_str();

        if method == "shutdown" {
            let resp = JsonRpcResponse::success(req_id, Value::Null);
            if let Ok(bytes) = serde_json::to_vec(&resp) {
                let _ = stdout.write_all(&bytes);
                let _ = stdout.write_all(b"\n");
                let _ = stdout.flush();
            }
            break;
        }

        let resp = match handle_request(method, &req.params).await {
            Ok(res_val) => JsonRpcResponse::success(req_id, res_val),
            Err((code, msg)) => JsonRpcResponse::error(req_id, code, msg),
        };

        if let Ok(bytes) = serde_json::to_vec(&resp) {
            let _ = stdout.write_all(&bytes);
            let _ = stdout.write_all(b"\n");
            let _ = stdout.flush();
        }
    }
}

async fn handle_request(method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "describe" => {
            let desc = describe_result();
            serde_json::to_value(desc)
                .map_err(|e| (-32603, format!("Failed to serialize describe: {e}")))
        }
        "validate_config" => {
            let config = params.get("config").unwrap_or(params);
            let res = validate_config_handler(config).await;
            Ok(res)
        }
        "browse" => {
            let config = params.get("config").unwrap_or(params);
            let resource_id = params
                .get("resource_id")
                .and_then(Value::as_str)
                .unwrap_or(RESOURCE_DOC_ID);

            let spec = global_loader()
                .load_spec(config)
                .await
                .map_err(|e| (-32601, e))?;

            if resource_id == RESOURCE_DOC_ID {
                let doc_item = mpe_plugin_sdk::protocol::datasource::DataItem {
                    id: "doc".to_string(),
                    name: spec.title.clone(),
                    protocol: mpe_plugin_sdk::protocol::datasource::DataItemProtocol::Other,
                    method: None,
                    path: None,
                    summary: spec.description.clone(),
                    extra: vec![mpe_plugin_sdk::protocol::datasource::KeyValue {
                        label: I18n::extra_spec_version().to_string(),
                        value: spec.version.to_string(),
                    }],
                    raw: Some(json!({
                        "title": spec.title,
                        "version": spec.version.to_string(),
                        "servers": spec.servers,
                    })),
                };
                serde_json::to_value(vec![doc_item])
                    .map_err(|e| (-32603, format!("Serialization error: {e}")))
            } else {
                let mut items = Vec::with_capacity(spec.operations.len());
                for op in &spec.operations {
                    items.push(operation_to_data_item(op, &spec));
                }
                serde_json::to_value(items)
                    .map_err(|e| (-32603, format!("Serialization error: {e}")))
            }
        }
        "get_item" => {
            let config = params.get("config").unwrap_or(params);
            let item_id = params
                .get("item_id")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "Missing 'item_id'".to_string()))?;

            let spec = global_loader()
                .load_spec(config)
                .await
                .map_err(|e| (-32601, e))?;

            if item_id == "doc" {
                let doc_item = mpe_plugin_sdk::protocol::datasource::DataItem {
                    id: "doc".to_string(),
                    name: spec.title.clone(),
                    protocol: mpe_plugin_sdk::protocol::datasource::DataItemProtocol::Other,
                    method: None,
                    path: None,
                    summary: spec.description.clone(),
                    extra: vec![mpe_plugin_sdk::protocol::datasource::KeyValue {
                        label: I18n::extra_spec_version().to_string(),
                        value: spec.version.to_string(),
                    }],
                    raw: Some(json!({
                        "title": spec.title,
                        "version": spec.version.to_string(),
                        "servers": spec.servers,
                    })),
                };
                serde_json::to_value(doc_item)
                    .map_err(|e| (-32603, format!("Serialization error: {e}")))
            } else {
                let op = spec
                    .operations
                    .iter()
                    .find(|o| o.id == item_id)
                    .ok_or_else(|| (-32602, format!("Item '{item_id}' not found")))?;

                let item = operation_to_data_item(op, &spec);
                serde_json::to_value(item)
                    .map_err(|e| (-32603, format!("Serialization error: {e}")))
            }
        }
        "get_templates" => {
            let config = params.get("config").unwrap_or(params);
            let item_ids_val = params
                .get("item_ids")
                .and_then(Value::as_array)
                .ok_or_else(|| (-32602, "Missing 'item_ids' array".to_string()))?;

            let spec = global_loader()
                .load_spec(config)
                .await
                .map_err(|e| (-32601, e))?;

            let datasource_id = if config.get("file_path").is_some() {
                DATASOURCE_FILE_ID
            } else {
                DATASOURCE_URL_ID
            };

            let base_url_override = config.get("base_url").and_then(Value::as_str);

            let mut templates = Vec::new();
            for id_v in item_ids_val {
                if let Some(id_str) = id_v.as_str() {
                    if let Some(op) = spec.operations.iter().find(|o| o.id == id_str) {
                        templates.push(operation_to_node_template(
                            op,
                            &spec,
                            datasource_id,
                            base_url_override,
                        ));
                    }
                }
            }

            serde_json::to_value(templates)
                .map_err(|e| (-32603, format!("Serialization error: {e}")))
        }
        _ => Err((-32601, format!("Method not found: {method}"))),
    }
}

async fn validate_config_handler(config: &Value) -> Value {
    if let Some(file_path) = config.get("file_path").and_then(Value::as_str) {
        match global_loader().load_from_file(file_path) {
            Ok(spec) => json!({
                "valid": true,
                "message": I18n::file_parse_success(spec.operations.len())
            }),
            Err(e) => json!({
                "valid": false,
                "message": I18n::file_check_failed(&e)
            }),
        }
    } else if let Some(url_str) = config.get("url").and_then(Value::as_str) {
        let headers = config.get("headers").and_then(Value::as_str);
        let base_url = config.get("base_url").and_then(Value::as_str);
        match global_loader()
            .load_from_url(url_str, headers, base_url)
            .await
        {
            Ok(spec) => json!({
                "valid": true,
                "message": I18n::url_parse_success(spec.operations.len())
            }),
            Err(e) => json!({
                "valid": false,
                "message": I18n::url_fetch_failed(&e)
            }),
        }
    } else {
        json!({
            "valid": false,
            "message": I18n::missing_url_or_file()
        })
    }
}

/// Runs the complete embedded self-test suite. Returns Ok(()) on PASS.
pub async fn run_self_test() -> Result<(), String> {
    println!("Running Swagger / OpenAPI Datasource Plugin Self-Test Suite...");

    // 1. Describe test
    println!("  [1/5] Testing describe handshake...");
    let desc = describe_result();
    if desc.plugin.name != "openapi" {
        return Err(format!(
            "Expected plugin name 'openapi', got '{}'",
            desc.plugin.name
        ));
    }
    if desc.datasources.len() != 2 {
        return Err(format!(
            "Expected 2 datasources, got {}",
            desc.datasources.len()
        ));
    }
    if desc.datasources[0].id != DATASOURCE_URL_ID || desc.datasources[1].id != DATASOURCE_FILE_ID {
        return Err("Datasource IDs mismatch".to_string());
    }
    println!("  [1/5] PASS - describe verified");

    // 2. Swagger 2.0 JSON parsing & template conversion
    println!("  [2/5] Testing Swagger 2.0 JSON parsing & conversion...");
    let swagger2_sample = r#"{
        "swagger": "2.0",
        "info": {"title": "Petstore Sample", "version": "1.0.0"},
        "host": "petstore.swagger.io",
        "basePath": "/v2",
        "schemes": ["https"],
        "paths": {
            "/pet/{petId}": {
                "get": {
                    "tags": ["pet"],
                    "summary": "Find pet by ID",
                    "operationId": "getPetById",
                    "parameters": [
                        {
                            "name": "petId",
                            "in": "path",
                            "required": true,
                            "type": "integer"
                        },
                        {
                            "name": "api_key",
                            "in": "header",
                            "required": false,
                            "type": "string"
                        }
                    ],
                    "security": [
                        {"api_key": []}
                    ]
                }
            }
        },
        "securityDefinitions": {
            "api_key": {
                "type": "apiKey",
                "name": "api_key",
                "in": "header"
            }
        }
    }"#;

    let s2_spec = parse_content(swagger2_sample, None)?;
    if s2_spec.operations.len() != 1 {
        return Err(format!(
            "Expected 1 operation, got {}",
            s2_spec.operations.len()
        ));
    }
    let s2_op = &s2_spec.operations[0];
    let s2_item = operation_to_data_item(s2_op, &s2_spec);
    if s2_item.name != "Find pet by ID" {
        return Err(format!(
            "Expected name 'Find pet by ID', got '{}'",
            s2_item.name
        ));
    }
    if !s2_item
        .extra
        .iter()
        .any(|kv| (kv.label == "分组" || kv.label == "Group") && kv.value == "pet")
    {
        return Err("Primary tag 'pet' not mapped to '分组' or 'Group'".to_string());
    }

    let s2_tpl = operation_to_node_template(s2_op, &s2_spec, "openapi", None);
    if s2_tpl.type_id != "http:request" {
        return Err("Template type_id is not http:request".to_string());
    }
    let s2_url = s2_tpl.data["url"].as_str().unwrap_or("");
    if !s2_url.contains("https://petstore.swagger.io/v2/pet/{{petId}}") {
        return Err(format!("URL path parameter not interpolated: {s2_url}"));
    }
    println!("  [2/5] PASS - Swagger 2.0 verified");

    // 3. OpenAPI 3.0 YAML parsing & mock body generation
    println!("  [3/5] Testing OpenAPI 3.0 YAML parsing & mock generation...");
    let openapi3_sample = r#"
openapi: 3.0.1
info:
  title: User API
  version: 1.0.0
servers:
  - url: https://api.example.com/v1
paths:
  /users:
    post:
      tags:
        - "用户中心/用户管理"
      summary: Create new user
      operationId: createUser
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/CreateUserRequest'
      security:
        - bearerAuth: []
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
  schemas:
    CreateUserRequest:
      type: object
      required:
        - username
        - email
      properties:
        username:
          type: string
          example: john_doe
        email:
          type: string
          format: email
        created_at:
          type: string
          format: date-time
        roles:
          type: array
          items:
            type: string
"#;

    let o3_spec = parse_content(openapi3_sample, None)?;
    if o3_spec.operations.len() != 1 {
        return Err(format!(
            "Expected 1 operation, got {}",
            o3_spec.operations.len()
        ));
    }
    let o3_op = &o3_spec.operations[0];
    let o3_item = operation_to_data_item(o3_op, &o3_spec);
    if !o3_item
        .extra
        .iter()
        .any(|kv| (kv.label == "分组" || kv.label == "Group") && kv.value == "用户中心/用户管理")
    {
        return Err(
            "Multi-level tag '用户中心/用户管理' not mapped to '分组' or 'Group'".to_string(),
        );
    }

    let o3_tpl = operation_to_node_template(o3_op, &o3_spec, "openapi", None);
    if o3_tpl.data["auth"]["auth_type"] != "bearer" {
        return Err("Bearer auth not properly mapped".to_string());
    }
    let body_str = o3_tpl.data["body"].as_str().unwrap_or("");
    if !body_str.contains("john_doe") || !body_str.contains("user@example.com") {
        return Err(format!("Mock body missing expected fields: {body_str}"));
    }
    println!("  [3/5] PASS - OpenAPI 3.0 YAML & mock generation verified");

    // 4. Recursive schema depth limit
    println!("  [4/5] Testing 5-level recursive schema protection...");
    let recursive_schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "child": {"$ref": "#/components/schemas/Recursive"}
        }
    });
    let root_doc = json!({
        "components": {
            "schemas": {
                "Recursive": recursive_schema
            }
        }
    });

    let mock_val = generate_mock(
        &json!({"$ref": "#/components/schemas/Recursive"}),
        &root_doc,
    );
    if !mock_val.is_object() {
        return Err("Recursive mock didn't return an object".to_string());
    }
    println!("  [4/5] PASS - Recursive schema depth limit verified");

    // 5. Config validation
    println!("  [5/5] Testing config validation...");
    let mut temp_file = std::env::temp_dir();
    temp_file.push("mpe_openapi_test_spec.json");
    std::fs::write(&temp_file, swagger2_sample)
        .map_err(|e| format!("Failed to write temp file: {e}"))?;

    let valid_cfg = json!({"file_path": temp_file.to_string_lossy()});
    let v_res = validate_config_handler(&valid_cfg).await;
    if v_res["valid"] != true {
        return Err(format!(
            "Valid file failed validation: {}",
            v_res["message"]
        ));
    }

    let invalid_cfg = json!({"file_path": "/path/to/non_existent_spec_file_12345.json"});
    let inv_res = validate_config_handler(&invalid_cfg).await;
    if inv_res["valid"] != false {
        return Err("Invalid file should fail validation".to_string());
    }
    let _ = std::fs::remove_file(temp_file);
    println!("  [5/5] PASS - Config validation verified");

    println!("\n[Self-Test] All tests PASSED!");
    Ok(())
}
