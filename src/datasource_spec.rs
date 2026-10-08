//! Datasource specification and describe definitions for Swagger / OpenAPI plugin.

use mpe_plugin_sdk::protocol::datasource::{
    DatasourceAuth, DatasourceDescription, DatasourceResource,
};
use mpe_plugin_sdk::protocol::describe::DescribeResult;
use mpe_plugin_sdk::protocol::manifest::{
    PluginCapabilities, PluginEntry, PluginKind, PluginManifest,
};
use serde_json::json;
use std::collections::HashMap;

use crate::i18n::I18n;

pub const DATASOURCE_URL_ID: &str = "openapi";
pub const DATASOURCE_FILE_ID: &str = "openapi_file";
pub const RESOURCE_DOC_ID: &str = "document";
pub const RESOURCE_API_ID: &str = "api";
/// Returns the Swagger / OpenAPI (URL) datasource definition.
pub fn openapi_url_datasource() -> DatasourceDescription {
    DatasourceDescription {
        id: DATASOURCE_URL_ID.to_string(),
        display_name: I18n::openapi_url_display_name().to_string(),
        auth: DatasourceAuth {
            r#type: "none".to_string(),
        },
        config_schema: Some(json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "title": I18n::url_title()
                },
                "headers": {
                    "type": "string",
                    "title": I18n::headers_title()
                },
                "base_url": {
                    "type": "string",
                    "title": I18n::url_base_url_title()
                }
            },
            "required": ["url"]
        })),
        resources: vec![
            DatasourceResource {
                id: RESOURCE_DOC_ID.to_string(),
                label: I18n::document_resource().to_string(),
                children: Some(RESOURCE_API_ID.to_string()),
                leaf: false,
            },
            DatasourceResource {
                id: RESOURCE_API_ID.to_string(),
                label: I18n::resource_label().to_string(),
                children: None,
                leaf: true,
            },
        ],
    }
}

/// Returns the Swagger / OpenAPI (本地文件) datasource definition.
pub fn openapi_file_datasource() -> DatasourceDescription {
    DatasourceDescription {
        id: DATASOURCE_FILE_ID.to_string(),
        display_name: I18n::openapi_file_display_name().to_string(),
        auth: DatasourceAuth {
            r#type: "none".to_string(),
        },
        config_schema: Some(json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "title": I18n::file_path_title()
                },
                "base_url": {
                    "type": "string",
                    "title": I18n::file_base_url_title()
                }
            },
            "required": ["file_path"]
        })),
        resources: vec![
            DatasourceResource {
                id: RESOURCE_DOC_ID.to_string(),
                label: I18n::document_resource().to_string(),
                children: Some(RESOURCE_API_ID.to_string()),
                leaf: false,
            },
            DatasourceResource {
                id: RESOURCE_API_ID.to_string(),
                label: I18n::resource_label().to_string(),
                children: None,
                leaf: true,
            },
        ],
    }
}

/// Returns the complete DescribeResult for handshake with host.
pub fn describe_result() -> DescribeResult {
    DescribeResult {
        plugin: PluginManifest {
            name: "openapi".to_string(),
            version: "0.1.0".to_string(),
            kind: PluginKind::Datasource,
            author: Some("multi-protocol-flow".to_string()),
            min_host_version: Some("0.1.0".to_string()),
            description: Some(I18n::plugin_description().to_string()),
            entry: PluginEntry {
                command: "./openapi".to_string(),
                args: vec![],
            },
            env: HashMap::new(),
            permissions: vec!["http.request".to_string()],
            capabilities: PluginCapabilities {
                streaming: false,
                single_node: false,
            },
            locales: vec!["zh-CN".to_string(), "en-US".to_string()],
            default_locale: Some("zh-CN".to_string()),
        },
        nodes: vec![],
        datasources: vec![openapi_url_datasource(), openapi_file_datasource()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_describe_result_structure() {
        let desc = describe_result();
        assert_eq!(desc.plugin.name, "openapi");
        assert_eq!(desc.plugin.kind, PluginKind::Datasource);
        assert_eq!(desc.datasources.len(), 2);
        assert_eq!(desc.datasources[0].id, "openapi");
        assert_eq!(desc.datasources[1].id, "openapi_file");
        assert!(desc.nodes.is_empty());
    }
}
