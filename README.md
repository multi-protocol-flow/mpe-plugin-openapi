# Swagger / OpenAPI Datasource Plugin (`mpe-plugin-openapi`)

Swagger and OpenAPI datasource plugin for [Multi-Protocol Flow Executor (MPE)](https://github.com/multi-protocol-flow/multi-protocol-flow-executor).

## Features

- **Dual Datasource Mode**:
  - `openapi`: Remote Swagger / OpenAPI document via HTTP/HTTPS URL.
  - `openapi_file`: Local Swagger / OpenAPI file (`.json`, `.yaml`, `.yml`).
- **Format Auto-Detection**: Supports both JSON and YAML documents automatically.
- **Specification Support**:
  - Swagger 2.0
  - OpenAPI 3.0.x / 3.1.x
- **Smart Grouping**: Automatically organizes API operations into multi-level folders by primary tags.
- **Mock Body Generation**:
  - Deep schema traversal (up to 5 levels)
  - Circular reference protection
  - Semantic mocks for formats (`date-time`, `email`, `uuid`, etc.)
- **One-Click Node Template**:
  - Automatically converts operations into MPE `http:request` canvas nodes.
  - Converts Swagger path params `{id}` into MPE variable format `{{id}}`.
  - Injects default query params, headers, authentication, and mock JSON body.

## CLI Usage

```bash
# Self-test mode
./openapi --self-test
```
