//! Normalized OpenAPI / Swagger AST models.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Specification version enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecVersion {
    Swagger20,
    OpenApi30(String),
    OpenApi31(String),
    Unknown(String),
}

impl fmt::Display for SpecVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Swagger20 => write!(f, "Swagger 2.0"),
            Self::OpenApi30(v) => write!(f, "OpenAPI {}", v),
            Self::OpenApi31(v) => write!(f, "OpenAPI {}", v),
            Self::Unknown(v) => write!(f, "Unknown ({})", v),
        }
    }
}

/// Normalized root specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedSpec {
    pub version: SpecVersion,
    pub title: String,
    pub description: Option<String>,
    pub servers: Vec<String>,
    pub operations: Vec<NormalizedOperation>,
    pub raw: Value,
}

/// Parameter location in HTTP request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterLocation {
    Path,
    Query,
    Header,
    Cookie,
    Body, // Swagger 2.0 body param fallback
}

impl std::str::FromStr for ParameterLocation {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "path" => Self::Path,
            "query" => Self::Query,
            "header" => Self::Header,
            "cookie" => Self::Cookie,
            "body" => Self::Body,
            _ => Self::Query,
        })
    }
}

impl ParameterLocation {
    pub fn parse_location(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "path" => Self::Path,
            "query" => Self::Query,
            "header" => Self::Header,
            "cookie" => Self::Cookie,
            "body" => Self::Body,
            _ => Self::Query,
        }
    }
}

/// Normalized parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedParameter {
    pub name: String,
    pub location: ParameterLocation,
    pub required: bool,
    pub description: Option<String>,
    pub schema: Option<Value>,
}

/// Normalized request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedRequestBody {
    pub content_type: String,
    pub schema: Option<Value>,
    pub example: Option<Value>,
    pub required: bool,
}

/// Normalized security requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedSecurityRequirement {
    pub scheme_name: String,
    pub scheme_type: String,           // "apiKey", "http", "oauth2"
    pub scheme_in: Option<String>,     // "header", "query"
    pub scheme_scheme: Option<String>, // "bearer", "basic"
    pub scopes: Vec<String>,
}

/// Normalized API operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedOperation {
    /// Unique identifier for this operation, e.g. "GET /users/{id}" or "/users/{id}:get"
    pub id: String,
    pub path: String,
    pub method: String, // Upper-case "GET", "POST", etc.
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub deprecated: bool,
    pub parameters: Vec<NormalizedParameter>,
    pub request_body: Option<NormalizedRequestBody>,
    pub security: Vec<NormalizedSecurityRequirement>,
}
