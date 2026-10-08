//! Spec loader, format sniffer (JSON/YAML), and caching layer.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;

use super::model::NormalizedSpec;
use super::openapi3::parse_openapi3;
use super::swagger2::parse_swagger2;

const URL_CACHE_TTL: Duration = Duration::from_secs(60);

struct FileCacheEntry {
    mtime: SystemTime,
    spec: NormalizedSpec,
}

struct UrlCacheEntry {
    cached_at: Instant,
    spec: NormalizedSpec,
}

#[derive(Default)]
pub struct SpecLoader {
    file_cache: RwLock<HashMap<String, FileCacheEntry>>,
    url_cache: RwLock<HashMap<String, UrlCacheEntry>>,
}

static GLOBAL_LOADER: std::sync::LazyLock<Arc<SpecLoader>> =
    std::sync::LazyLock::new(|| Arc::new(SpecLoader::default()));

pub fn global_loader() -> Arc<SpecLoader> {
    GLOBAL_LOADER.clone()
}

impl SpecLoader {
    /// Loads a specification based on datasource config.
    pub async fn load_spec(&self, config: &Value) -> Result<NormalizedSpec, String> {
        if let Some(file_path) = config.get("file_path").and_then(Value::as_str) {
            self.load_from_file(file_path)
        } else if let Some(url_str) = config.get("url").and_then(Value::as_str) {
            let headers = config.get("headers").and_then(Value::as_str);
            let base_url = config.get("base_url").and_then(Value::as_str);
            self.load_from_url(url_str, headers, base_url).await
        } else {
            Err("Neither 'url' nor 'file_path' provided in datasource config".to_string())
        }
    }

    /// Loads and parses a specification from local file with mtime caching.
    pub fn load_from_file(&self, file_path: &str) -> Result<NormalizedSpec, String> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Err(format!("File does not exist: {file_path}"));
        }

        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to read metadata for {file_path}: {e}"))?;
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        // Check cache
        if let Ok(cache) = self.file_cache.read() {
            if let Some(entry) = cache.get(file_path) {
                if entry.mtime == mtime {
                    return Ok(entry.spec.clone());
                }
            }
        }

        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file {file_path}: {e}"))?;

        let spec = parse_content(&content, None)?;

        // Update cache
        if let Ok(mut cache) = self.file_cache.write() {
            cache.insert(
                file_path.to_string(),
                FileCacheEntry {
                    mtime,
                    spec: spec.clone(),
                },
            );
        }

        Ok(spec)
    }

    /// Loads and parses a specification from a remote URL with 60-second TTL caching.
    pub async fn load_from_url(
        &self,
        url_str: &str,
        headers_json: Option<&str>,
        base_url_override: Option<&str>,
    ) -> Result<NormalizedSpec, String> {
        let cache_key = format!("{url_str}:{}", headers_json.unwrap_or(""));

        // Check cache
        if let Ok(cache) = self.url_cache.read() {
            if let Some(entry) = cache.get(&cache_key) {
                if entry.cached_at.elapsed() < URL_CACHE_TTL {
                    let mut cached_spec = entry.spec.clone();
                    if let Some(override_base) = base_url_override {
                        if !override_base.trim().is_empty() {
                            cached_spec
                                .servers
                                .insert(0, override_base.trim().to_string());
                        }
                    }
                    return Ok(cached_spec);
                }
            }
        }

        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let mut req_builder = client.get(url_str);

        if let Some(h_json) = headers_json {
            if !h_json.trim().is_empty() {
                if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(h_json) {
                    for (k, v) in map {
                        if let Some(v_str) = v.as_str() {
                            req_builder = req_builder.header(&k, v_str);
                        }
                    }
                }
            }
        }

        let resp = req_builder
            .send()
            .await
            .map_err(|e| format!("Failed to fetch URL {url_str}: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!(
                "Failed to fetch {url_str}: HTTP status {}",
                resp.status()
            ));
        }

        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body from {url_str}: {e}"))?;

        // Determine effective doc_base_url for resolving relative servers in OpenAPI 3
        let doc_base = base_url_override.or(Some(url_str));
        let mut spec = parse_content(&text, doc_base)?;

        if let Some(override_base) = base_url_override {
            if !override_base.trim().is_empty() {
                spec.servers.insert(0, override_base.trim().to_string());
            }
        }

        // Update cache
        if let Ok(mut cache) = self.url_cache.write() {
            cache.insert(
                cache_key,
                UrlCacheEntry {
                    cached_at: Instant::now(),
                    spec: spec.clone(),
                },
            );
        }

        Ok(spec)
    }
}

/// Automatically sniffs format (JSON or YAML) and parses into NormalizedSpec.
pub fn parse_content(raw_str: &str, doc_base_url: Option<&str>) -> Result<NormalizedSpec, String> {
    let trimmed = raw_str.trim();
    if trimmed.is_empty() {
        return Err("Specification content is empty".to_string());
    }

    // 1. Try parsing as JSON first
    let root_val: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => {
            // 2. Fall back to YAML parsing into Value
            serde_yml::from_str(trimmed)
                .map_err(|e| format!("Failed to parse as either JSON or YAML: {e}"))?
        }
    };

    if !root_val.is_object() {
        return Err("Root of specification must be a JSON/YAML object".to_string());
    }

    // 3. Dispatch based on version field
    if let Some(swagger_ver) = root_val.get("swagger").and_then(Value::as_str) {
        if swagger_ver == "2.0" {
            return parse_swagger2(&root_val);
        }
    }

    if let Some(openapi_ver) = root_val.get("openapi").and_then(Value::as_str) {
        if openapi_ver.starts_with("3.") {
            return parse_openapi3(&root_val, doc_base_url);
        }
    }

    Err("Document is neither Swagger 2.0 nor OpenAPI 3.x (missing 'swagger: \"2.0\"' or 'openapi: \"3.x\"')".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_content_json_swagger2() {
        let json_str = r#"{
            "swagger": "2.0",
            "info": {"title": "Test Swagger", "version": "1.0"},
            "paths": {
                "/pets": {
                    "get": {
                        "summary": "List pets",
                        "responses": {"200": {"description": "OK"}}
                    }
                }
            }
        }"#;

        let spec = parse_content(json_str, None).unwrap();
        assert_eq!(spec.title, "Test Swagger");
        assert_eq!(spec.operations.len(), 1);
        assert_eq!(spec.operations[0].path, "/pets");
        assert_eq!(spec.operations[0].method, "GET");
    }

    #[test]
    fn test_parse_content_yaml_openapi3() {
        let yaml_str = r#"
openapi: "3.0.3"
info:
  title: "Test OpenAPI YAML"
  version: "1.0.0"
servers:
  - url: "https://api.example.com/v1"
paths:
  /users/{id}:
    get:
      summary: "Get user by ID"
      parameters:
        - name: "id"
          in: "path"
          required: true
          schema:
            type: "string"
      responses:
        '200':
          description: "User details"
"#;

        let spec = parse_content(yaml_str, None).unwrap();
        assert_eq!(spec.title, "Test OpenAPI YAML");
        assert_eq!(spec.servers[0], "https://api.example.com/v1");
        assert_eq!(spec.operations.len(), 1);
        assert_eq!(spec.operations[0].path, "/users/{id}");
        assert_eq!(spec.operations[0].parameters.len(), 1);
    }
}
