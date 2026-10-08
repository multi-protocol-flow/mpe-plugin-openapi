//! Multi-language (i18n) support for Swagger / OpenAPI datasource plugin.

use std::env;

/// Returns the effective locale from `MPE_LOCALE`, defaulting to `"zh-CN"`.
pub fn locale() -> String {
    env::var("MPE_LOCALE").unwrap_or_else(|_| "zh-CN".to_string())
}

/// Returns true if the active locale is Chinese (`zh-CN`, `zh`, etc.).
pub fn is_zh() -> bool {
    let loc = locale();
    loc.starts_with("zh")
}

pub struct I18n;

impl I18n {
    pub fn openapi_url_display_name() -> &'static str {
        "Swagger / OpenAPI (URL)"
    }

    pub fn openapi_file_display_name() -> &'static str {
        if is_zh() {
            "Swagger / OpenAPI (本地文件)"
        } else {
            "Swagger / OpenAPI (Local File)"
        }
    }

    pub fn plugin_description() -> &'static str {
        if is_zh() {
            "Swagger / OpenAPI 数据源（在线 URL + 本地规范文件导入）"
        } else {
            "Swagger / OpenAPI data source (online URL + local spec file import)"
        }
    }

    pub fn document_resource() -> &'static str {
        if is_zh() {
            "文档"
        } else {
            "Document"
        }
    }

    pub fn resource_label() -> &'static str {
        if is_zh() {
            "接口"
        } else {
            "APIs"
        }
    }

    pub fn url_title() -> &'static str {
        if is_zh() {
            "文档地址 (URL)"
        } else {
            "Document URL"
        }
    }

    pub fn headers_title() -> &'static str {
        if is_zh() {
            "请求头 (JSON)"
        } else {
            "Headers (JSON)"
        }
    }

    pub fn url_base_url_title() -> &'static str {
        if is_zh() {
            "服务地址 (Base URL)"
        } else {
            "Base URL"
        }
    }

    pub fn file_path_title() -> &'static str {
        if is_zh() {
            "规范文件路径"
        } else {
            "File Path"
        }
    }

    pub fn file_base_url_title() -> &'static str {
        if is_zh() {
            "服务地址 (Base URL)"
        } else {
            "Base URL"
        }
    }

    pub fn file_parse_success(count: usize) -> String {
        if is_zh() {
            format!("文件解析成功，包含 {} 个接口", count)
        } else {
            format!("File parsed successfully, contains {} operation(s)", count)
        }
    }

    pub fn file_check_failed(e: &str) -> String {
        if is_zh() {
            format!("文件校验失败: {e}")
        } else {
            format!("File validation failed: {e}")
        }
    }

    pub fn url_parse_success(count: usize) -> String {
        if is_zh() {
            format!("在线规范拉取并解析成功，包含 {} 个接口", count)
        } else {
            format!(
                "Specification fetched and parsed successfully, contains {} operation(s)",
                count
            )
        }
    }

    pub fn url_fetch_failed(e: &str) -> String {
        if is_zh() {
            format!("URL 连接或解析失败: {e}")
        } else {
            format!("URL fetch or parse failed: {e}")
        }
    }

    pub fn missing_url_or_file() -> &'static str {
        if is_zh() {
            "配置中缺少 'url' 或 'file_path'"
        } else {
            "Missing 'url' or 'file_path' in configuration"
        }
    }

    pub fn extra_group() -> &'static str {
        if is_zh() {
            "分组"
        } else {
            "Group"
        }
    }

    pub fn extra_spec_version() -> &'static str {
        if is_zh() {
            "规范版本"
        } else {
            "Spec Version"
        }
    }

    pub fn extra_deprecated() -> &'static str {
        if is_zh() {
            "已废弃"
        } else {
            "Deprecated"
        }
    }

    pub fn yes() -> &'static str {
        if is_zh() {
            "是"
        } else {
            "Yes"
        }
    }
}
