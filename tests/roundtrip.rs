//! Offline stdio JSON-RPC roundtrip tests against the compiled `openapi` binary.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const PLUGIN_BIN: &str = env!("CARGO_BIN_EXE_openapi");

struct PluginProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl PluginProcess {
    fn spawn() -> Self {
        let mut child = Command::new(PLUGIN_BIN)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn plugin process");

        let stdin = child.stdin.take().expect("pipe stdin");
        let stdout = BufReader::new(child.stdout.take().expect("pipe stdout"));

        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut line = serde_json::to_string(&req).expect("serialize request");
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .expect("write request");
        self.stdin.flush().expect("flush request");

        let mut resp_line = String::new();
        self.stdout
            .read_line(&mut resp_line)
            .expect("read response line");

        serde_json::from_str(&resp_line).expect("parse response JSON")
    }

    fn finish(mut self) -> std::process::ExitStatus {
        drop(self.stdin);
        self.child.wait().expect("child exit")
    }
}

#[test]
fn test_rpc_describe() {
    let mut proc = PluginProcess::spawn();
    let resp = proc.request(1, "describe", json!({}));
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);

    let result = &resp["result"];
    assert_eq!(result["plugin"]["name"], "openapi");
    assert_eq!(result["plugin"]["kind"], "datasource");

    let datasources = result["datasources"].as_array().expect("datasources array");
    assert_eq!(datasources.len(), 2);
    assert_eq!(datasources[0]["id"], "openapi");
    assert_eq!(datasources[1]["id"], "openapi_file");

    let status = proc.finish();
    assert!(status.success());
}

#[test]
fn test_rpc_validate_config() {
    let mut proc = PluginProcess::spawn();

    // 1. Invalid file
    let invalid_resp = proc.request(
        2,
        "validate_config",
        json!({"config": {"file_path": "/path/to/missing_file.json"}}),
    );
    assert_eq!(invalid_resp["result"]["valid"], false);

    // 2. Valid file
    let mut temp = std::env::temp_dir();
    temp.push("test_openapi_valid.json");
    let content = r#"{
        "swagger": "2.0",
        "info": {"title": "Test", "version": "1.0"},
        "paths": {
            "/ping": {"get": {"summary": "Ping"}}
        }
    }"#;
    std::fs::write(&temp, content).expect("write temp file");

    let valid_resp = proc.request(
        3,
        "validate_config",
        json!({"config": {"file_path": temp.to_string_lossy()}}),
    );
    assert_eq!(valid_resp["result"]["valid"], true);
    let _ = std::fs::remove_file(temp);

    let status = proc.finish();
    assert!(status.success());
}

#[test]
fn test_rpc_browse_and_templates() {
    let mut proc = PluginProcess::spawn();

    let yaml_doc = r#"
openapi: 3.0.0
info:
  title: Store API
  version: 1.0.0
servers:
  - url: https://api.store.com/v1
paths:
  /orders/{orderId}:
    get:
      tags:
        - "Orders/Query"
      summary: Get order by ID
      operationId: getOrder
      parameters:
        - name: orderId
          in: path
          required: true
          schema:
            type: string
        - name: format
          in: query
          schema:
            type: string
            default: json
      responses:
        '200':
          description: OK
"#;

    let mut temp = std::env::temp_dir();
    temp.push("test_store_openapi.yaml");
    std::fs::write(&temp, yaml_doc).expect("write temp yaml");

    let cfg = json!({"file_path": temp.to_string_lossy()});

    // 1. Browse
    let browse_resp = proc.request(10, "browse", json!({"config": cfg, "resource_id": "api"}));
    let items = browse_resp["result"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item["name"], "Get order by ID");
    assert_eq!(item["method"], "GET");
    assert_eq!(item["path"], "/orders/{orderId}");

    // Check extra grouping
    let extra = item["extra"].as_array().expect("extra array");
    let group_kv = extra
        .iter()
        .find(|kv| kv["label"] == "分组" || kv["label"] == "Group")
        .expect("group kv");
    assert_eq!(group_kv["value"], "Orders/Query");

    let item_id = item["id"].as_str().expect("item id");

    // 2. Get item
    let get_resp = proc.request(11, "get_item", json!({"config": cfg, "item_id": item_id}));
    assert_eq!(get_resp["result"]["id"], item_id);

    // 3. Get templates
    let tpl_resp = proc.request(
        12,
        "get_templates",
        json!({"config": cfg, "item_ids": [item_id]}),
    );
    let templates = tpl_resp["result"].as_array().expect("templates array");
    assert_eq!(templates.len(), 1);
    let tpl = &templates[0];
    assert_eq!(tpl["type_id"], "http:request");
    assert_eq!(tpl["name"], "Get order by ID");
    let data = &tpl["data"];
    assert_eq!(data["method"], "get");
    let url = data["url"].as_str().expect("template url");
    assert!(url.contains("https://api.store.com/v1/orders/{{orderId}}"));
    assert!(url.contains("format=json"));

    let _ = std::fs::remove_file(temp);

    // 4. Shutdown
    let shutdown_resp = proc.request(99, "shutdown", json!({}));
    assert!(shutdown_resp["result"].is_null());

    let status = proc.finish();
    assert!(status.success());
}
