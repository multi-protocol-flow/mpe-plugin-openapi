# Swagger / OpenAPI 数据源插件 (`mpe-plugin-openapi`)

用于 [Multi-Protocol Flow Executor (MPE)](https://github.com/multi-protocol-flow/multi-protocol-flow-executor) 的官方 Swagger / OpenAPI 数据源插件。

## 功能特性

- **双数据源模式**:
  - `openapi`: 在线文档（通过 HTTP/HTTPS URL 获取，支持自定义请求头与 Base URL 覆盖）
  - `openapi_file`: 本地规范文件（支持绝对与相对路径）
- **多格式自动嗅探**: 自动识别并解析 JSON 与 YAML (`.yaml`/`.yml`) 格式。
- **全版本规范兼容**:
  - Swagger 2.0
  - OpenAPI 3.0.x / 3.1.x
- **语义化目录树**: 根据接口 Tag 自动构建多级目录树，无缝集成宿主数据源面板折叠展示。
- **智能 Mock Body 生成**:
  - 递归解析 Schema 属性（严格限制最大深度 5 层）
  - 环形循环引用检测与安全截断防护
  - 常见格式类型语义填充（`date-time`, `email`, `uuid`, `enum`, `allOf`, `oneOf` 等）
- **一键落画布模板**:
  - 自动将接口转换为宿主标准 `http:request` 节点模板
  - 路径参数 `{param}` 自动映射为 MPE 插值语法 `{{param}}`
  - 自动补全 Query 占位参数、请求头、鉴权配置与 JSON 请求体。

## 命令行模式

```bash
# 运行单元与集成自测套件
./openapi --self-test
```
