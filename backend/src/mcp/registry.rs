//! The OpenAPI document, read back as a list of tools.
//!
//! `openapi::ApiDoc` is the one description of the API, generated from the
//! `#[utoipa::path]` attributes on the handlers. Walking it at startup means
//! every operation is a tool with nothing to declare a second time: a new
//! endpoint is a new tool the moment it is annotated, and a tool cannot
//! describe a request shape the API no longer accepts, because there is no
//! second copy of the shape to go stale. The client that once broke on a
//! renamed field had been keeping that copy by hand.
//!
//! The walk is over the document's JSON rather than utoipa's typed model.
//! The tool schema has to end up as JSON Schema anyway, and dereferencing
//! `$ref`s is a tree rewrite that is simplest on a `Value`.

use std::collections::{BTreeSet, HashMap};

use axum::http::Method;
use serde_json::{json, Map, Value};

/// One API operation, ready to be called through MCP.
#[derive(Debug, Clone)]
pub struct ApiTool {
    /// `{tag}_{operation_id}`. utoipa's default operation id is the bare
    /// handler name, and `list` exists in most modules, so the tag is what
    /// makes it unique.
    pub name: String,
    pub method: Method,
    /// The path as the router knows it, placeholders included:
    /// `/api/v1/diary/{id}`.
    pub path: String,
    pub description: String,
    pub path_params: Vec<String>,
    pub query_params: Vec<String>,
    pub body: BodyMode,
    /// JSON Schema for the tool's arguments, fully dereferenced.
    pub input_schema: Map<String, Value>,
}

/// How a request body appears in the tool's arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyMode {
    /// No request body.
    None,
    /// The body's fields sit beside the path and query parameters, so a call
    /// reads as one flat argument list. Used whenever no name collides.
    Flat,
    /// The body is an object under `body`, because one of its fields shares a
    /// name with a path or query parameter and flattening would lose one.
    Nested,
}

impl ApiTool {
    /// Whether calling this tool changes anything. Decided by the HTTP method,
    /// which is the same rule the API's key scopes use, so a read key sees
    /// exactly the tools it could successfully call.
    pub fn writes(&self) -> bool {
        !self.method.is_safe()
    }
}

/// Tags whose operations are never tools. Credentials and administration are
/// closed to API keys already; listing them would only offer tools that
/// answer 403 to everyone.
const EXCLUDED_TAGS: &[&str] = &["auth", "admin", "keys"];

/// Build the tool list from an OpenAPI document.
///
/// Skips excluded tags, multipart uploads (an MCP argument list has nowhere
/// to put file bytes) and operations whose success response is binary.
pub fn tools_from_openapi(doc: &Value) -> Vec<ApiTool> {
    let components = doc
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let Some(paths) = doc.get("paths").and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut tools = Vec::new();
    let mut names = BTreeSet::new();

    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for (method_name, operation) in item {
            let Ok(method) = Method::from_bytes(method_name.to_ascii_uppercase().as_bytes()) else {
                continue;
            };
            let Some(op) = operation.as_object() else {
                continue;
            };
            let Some(tool) = tool_from_operation(path, method, op, &components) else {
                continue;
            };
            // Two tools with one name would leave a client calling whichever
            // won, so a collision is reported rather than resolved quietly.
            if !names.insert(tool.name.clone()) {
                tracing::warn!(tool = %tool.name, "duplicate MCP tool name; skipping the later one");
                continue;
            }
            tools.push(tool);
        }
    }

    tools
}

fn tool_from_operation(
    path: &str,
    method: Method,
    op: &Map<String, Value>,
    components: &Map<String, Value>,
) -> Option<ApiTool> {
    let tag = op
        .get("tags")
        .and_then(Value::as_array)
        .and_then(|t| t.first())
        .and_then(Value::as_str)
        .unwrap_or("api");
    if EXCLUDED_TAGS.contains(&tag) {
        return None;
    }
    if responds_with_binary(op) {
        return None;
    }

    let operation_id = op.get("operationId").and_then(Value::as_str)?;
    let name = format!("{tag}_{operation_id}");

    let mut properties = Map::new();
    let mut required = Vec::new();
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();

    for param in op
        .get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(param) = param.as_object() else {
            continue;
        };
        let Some(pname) = param.get("name").and_then(Value::as_str) else {
            continue;
        };
        let location = param.get("in").and_then(Value::as_str).unwrap_or("query");
        let mut schema = param
            .get("schema")
            .cloned()
            .map(|s| dereference(s, components))
            .unwrap_or_else(|| json!({}));
        if let Some(desc) = param.get("description").and_then(Value::as_str) {
            if let Some(obj) = schema.as_object_mut() {
                obj.entry("description")
                    .or_insert_with(|| Value::String(desc.to_string()));
            }
        }
        match location {
            "path" => {
                path_params.push(pname.to_string());
                required.push(pname.to_string());
            }
            "query" => {
                query_params.push(pname.to_string());
                if param.get("required").and_then(Value::as_bool) == Some(true) {
                    required.push(pname.to_string());
                }
            }
            // Header and cookie parameters are not something a tool argument
            // should set, and the API declares none.
            _ => continue,
        }
        properties.insert(pname.to_string(), schema);
    }

    let mut body = BodyMode::None;
    if let Some(request_body) = op.get("requestBody").and_then(Value::as_object) {
        let content = request_body.get("content").and_then(Value::as_object)?;
        // Anything that is not JSON — today that is the multipart photo
        // uploads — has no representation as tool arguments.
        let (_, json_content) = content
            .iter()
            .find(|(mime, _)| mime.starts_with("application/json"))?;
        let schema = json_content
            .get("schema")
            .cloned()
            .map(|s| dereference(s, components))
            .unwrap_or_else(|| json!({"type": "object"}));
        let body_required = request_body.get("required").and_then(Value::as_bool) != Some(false);

        let body_props = schema.get("properties").and_then(Value::as_object).cloned();
        let collides = body_props
            .as_ref()
            .is_some_and(|props| props.keys().any(|k| properties.contains_key(k)));

        match body_props {
            Some(props) if !collides => {
                for (k, v) in props {
                    properties.insert(k, v);
                }
                if let Some(req) = schema.get("required").and_then(Value::as_array) {
                    required.extend(req.iter().filter_map(Value::as_str).map(String::from));
                }
                body = BodyMode::Flat;
            }
            _ => {
                properties.insert("body".to_string(), schema);
                if body_required {
                    required.push("body".to_string());
                }
                body = BodyMode::Nested;
            }
        }
    }

    let mut input_schema = Map::new();
    input_schema.insert("type".into(), json!("object"));
    input_schema.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        required.sort();
        required.dedup();
        input_schema.insert("required".into(), json!(required));
    }

    Some(ApiTool {
        name,
        description: describe(&method, path, op, body),
        method,
        path: path.to_string(),
        path_params,
        query_params,
        body,
        input_schema,
    })
}

/// A 2xx response whose content is an image or a raw byte stream. Those are
/// served for browsers and have no useful text form for a model.
fn responds_with_binary(op: &Map<String, Value>) -> bool {
    op.get("responses")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(status, _)| status.starts_with('2'))
        .filter_map(|(_, r)| r.get("content").and_then(Value::as_object))
        .flat_map(|content| content.keys())
        .any(|mime| mime.starts_with("image/") || mime == "application/octet-stream")
}

/// Most handlers carry no doc comment, so the summary alone would leave many
/// tools described as nothing. The method and path always exist, and the
/// success response's description is often the best sentence in the whole
/// annotation ("One day, grouped by meal, with targets").
fn describe(method: &Method, path: &str, op: &Map<String, Value>, body: BodyMode) -> String {
    let mut parts: Vec<String> = Vec::new();

    let summary = op.get("summary").and_then(Value::as_str).map(str::trim);
    parts.push(match summary {
        Some(s) if !s.is_empty() => s.trim_end_matches('.').to_string() + ".",
        _ => format!("{method} {}.", path.trim_start_matches("/api/v1")),
    });

    if let Some(desc) = op.get("description").and_then(Value::as_str) {
        let desc = desc.trim();
        if !desc.is_empty() {
            parts.push(desc.to_string());
        }
    }

    if let Some(success) = op
        .get("responses")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(status, _)| status.starts_with('2'))
        .filter_map(|(_, r)| r.get("description").and_then(Value::as_str))
        .map(str::trim)
        .find(|d| !d.is_empty())
    {
        parts.push(format!("Returns: {success}."));
    }

    if body == BodyMode::Nested {
        parts.push("The request body goes under `body`.".into());
    }
    if !method.is_safe() {
        parts.push("Needs a key with the write scope.".into());
    }

    parts.join(" ")
}

/// Replace every `$ref` in a schema with the component it points at, so a
/// client sees the whole shape inline. A reference back into a schema already
/// being expanded is left as-is, which is the only way to keep the output
/// finite; the API has none today.
fn dereference(schema: Value, components: &Map<String, Value>) -> Value {
    let mut stack = Vec::new();
    deref_inner(schema, components, &mut stack)
}

fn deref_inner(value: Value, components: &Map<String, Value>, stack: &mut Vec<String>) -> Value {
    match value {
        Value::Object(mut obj) => {
            if let Some(Value::String(reference)) = obj.get("$ref") {
                let name = reference
                    .strip_prefix("#/components/schemas/")
                    .unwrap_or(reference)
                    .to_string();
                if stack.contains(&name) {
                    return Value::Object(obj);
                }
                let Some(target) = components.get(&name) else {
                    tracing::warn!(reference = %name, "OpenAPI $ref points at nothing");
                    return Value::Object(obj);
                };
                stack.push(name);
                let mut resolved = deref_inner(target.clone(), components, stack);
                stack.pop();
                // Sibling keys beside a `$ref` (a description on the field,
                // say) are kept, over whatever the component said.
                obj.remove("$ref");
                if let (Some(resolved_obj), true) = (resolved.as_object_mut(), !obj.is_empty()) {
                    for (k, v) in obj {
                        resolved_obj.insert(k, v);
                    }
                }
                return resolved;
            }
            let rewritten: Map<String, Value> = obj
                .into_iter()
                .map(|(k, v)| (k, deref_inner(v, components, stack)))
                .collect();
            Value::Object(rewritten)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|v| deref_inner(v, components, stack))
                .collect(),
        ),
        other => other,
    }
}

/// Index tools by name for dispatch.
pub fn index(tools: &[ApiTool]) -> HashMap<String, usize> {
    tools
        .iter()
        .enumerate()
        .map(|(i, t)| (t.name.clone(), i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    fn doc() -> Value {
        serde_json::to_value(crate::openapi::ApiDoc::openapi()).unwrap()
    }

    fn find<'a>(tools: &'a [ApiTool], name: &str) -> &'a ApiTool {
        tools
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no tool named {name}"))
    }

    #[test]
    fn every_tool_has_a_unique_name_and_a_description() {
        let tools = tools_from_openapi(&doc());
        assert!(tools.len() > 30, "only {} tools", tools.len());
        let names: BTreeSet<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names.len(), tools.len());
        for tool in &tools {
            assert!(
                !tool.description.is_empty(),
                "{} has no description",
                tool.name
            );
            assert_eq!(tool.input_schema["type"], "object");
        }
    }

    #[test]
    fn credentials_and_administration_are_not_tools() {
        let tools = tools_from_openapi(&doc());
        for tool in &tools {
            assert!(
                !tool.path.contains("/auth/")
                    && !tool.path.contains("/keys")
                    && !tool.path.contains("/admin/"),
                "{} should not be exposed",
                tool.name
            );
        }
    }

    #[test]
    fn uploads_and_image_bytes_are_left_out() {
        let tools = tools_from_openapi(&doc());
        assert!(
            !tools.iter().any(|t| t.name.contains("upload")),
            "multipart uploads cannot be tool arguments"
        );
        assert!(
            !tools.iter().any(|t| t.name == "photos_serve"),
            "image bytes are not a tool result"
        );
        // The rest of the photo surface stays: captions and listings are JSON.
        find(&tools, "photos_set_caption");
    }

    #[test]
    fn path_parameters_are_required_and_query_parameters_are_optional() {
        let tools = tools_from_openapi(&doc());
        let day = find(&tools, "diary_day");
        assert_eq!(day.method, Method::GET);
        assert_eq!(day.query_params, vec!["date"]);
        assert!(day.input_schema.get("required").is_none());

        let one = find(&tools, "diary_get_one");
        assert_eq!(one.path_params, vec!["id"]);
        assert_eq!(one.input_schema["required"], json!(["id"]));
        assert_eq!(one.input_schema["properties"]["id"]["format"], "uuid");
    }

    #[test]
    fn a_json_body_is_flattened_into_the_arguments_with_refs_resolved() {
        let tools = tools_from_openapi(&doc());
        let create = find(&tools, "diary_create");
        assert_eq!(create.body, BodyMode::Flat);
        let props = create.input_schema["properties"].as_object().unwrap();
        assert!(props.contains_key("food_id"));
        assert!(props.contains_key("quantity_g"));

        // A nested component is inlined rather than left as a pointer.
        let recipe = find(&tools, "recipes_create");
        let items = &recipe.input_schema["properties"]["items"];
        assert_eq!(items["type"], "array");
        assert!(items["items"].get("$ref").is_none(), "$ref left unresolved");
        assert!(items["items"]["properties"].get("food_id").is_some());
        assert_eq!(
            recipe.input_schema["required"],
            json!(["items", "name", "servings"])
        );
    }

    #[test]
    fn a_path_parameter_beside_a_body_keeps_both() {
        let tools = tools_from_openapi(&doc());
        let update = find(&tools, "recipes_update");
        assert_eq!(update.method, Method::PUT);
        assert_eq!(update.path_params, vec!["id"]);
        assert_eq!(update.body, BodyMode::Flat);
        let props = update.input_schema["properties"].as_object().unwrap();
        assert!(props.contains_key("id"));
        assert!(props.contains_key("items"));
    }

    #[test]
    fn a_colliding_body_field_moves_the_body_under_its_own_key() {
        let doc = json!({
            "paths": {
                "/api/v1/things/{id}": {
                    "put": {
                        "tags": ["things"],
                        "operationId": "update",
                        "parameters": [{"name": "id", "in": "path", "required": true,
                                        "schema": {"type": "string"}}],
                        "requestBody": {"content": {"application/json": {"schema": {
                            "$ref": "#/components/schemas/Thing"}}}},
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            },
            "components": {"schemas": {"Thing": {
                "type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]
            }}}
        });
        let tools = tools_from_openapi(&doc);
        let tool = find(&tools, "things_update");
        assert_eq!(tool.body, BodyMode::Nested);
        assert_eq!(
            tool.input_schema["properties"]["body"]["properties"]["id"]["type"],
            "string"
        );
        assert_eq!(tool.input_schema["required"], json!(["body", "id"]));
        assert!(tool.description.contains("under `body`"));
    }

    #[test]
    fn a_cyclic_reference_terminates() {
        let doc = json!({
            "paths": {"/api/v1/nodes": {"post": {
                "tags": ["nodes"], "operationId": "create",
                "requestBody": {"content": {"application/json": {"schema": {
                    "$ref": "#/components/schemas/Node"}}}},
                "responses": {"201": {"description": "made"}}
            }}},
            "components": {"schemas": {"Node": {
                "type": "object",
                "properties": {"child": {"$ref": "#/components/schemas/Node"}}
            }}}
        });
        let tools = tools_from_openapi(&doc);
        let tool = find(&tools, "nodes_create");
        assert_eq!(
            tool.input_schema["properties"]["child"]["$ref"],
            "#/components/schemas/Node"
        );
    }

    #[test]
    fn reads_and_writes_follow_the_method() {
        let tools = tools_from_openapi(&doc());
        assert!(!find(&tools, "foods_list").writes());
        assert!(find(&tools, "foods_create").writes());
        assert!(find(&tools, "diary_delete").writes());
        assert!(find(&tools, "diary_delete")
            .description
            .contains("write scope"));
    }
}
