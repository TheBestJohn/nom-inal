//! Calling the API from inside the process.
//!
//! A tool call becomes an HTTP request and is handed to a clone of the same
//! axum router the network serves — no loopback socket, no second client.
//! What that buys is that the request meets exactly the API: the same
//! extractor authenticates the key and applies its scope to the method, the
//! same validators reject a bad body, and the same error type explains why.
//! The MCP layer adds nothing of its own to any of that, so it cannot
//! disagree with it.

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Map, Value};
use tower::ServiceExt;

use super::registry::{ApiTool, BodyMode};

/// The credential the MCP request arrived with, carried onto every API
/// request it makes. Kept as the raw header values so the API reads exactly
/// what the client sent.
#[derive(Debug, Clone, Default)]
pub struct Credential {
    authorization: Option<HeaderValue>,
    api_key: Option<HeaderValue>,
}

impl Credential {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            authorization: headers.get(header::AUTHORIZATION).cloned(),
            api_key: headers.get("x-api-key").cloned(),
        }
    }
}

/// What came back, with the body already decoded into something a model can
/// read.
#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: StatusCode,
    pub body: Value,
}

impl ApiResponse {
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }
}

/// Largest response body a tool will read. The food export is the biggest
/// thing the API returns and it is a few megabytes at most.
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct Dispatcher {
    router: Router,
}

impl Dispatcher {
    pub fn new(router: Router) -> Self {
        Self { router }
    }

    /// Send one request through the router.
    pub async fn send(
        &self,
        credential: &Credential,
        method: Method,
        path_and_query: &str,
        body: Option<&Value>,
    ) -> Result<ApiResponse, String> {
        let mut builder = Request::builder()
            .method(method)
            .uri(path_and_query)
            .header(header::ACCEPT, "application/json, text/event-stream");
        if let Some(auth) = &credential.authorization {
            builder = builder.header(header::AUTHORIZATION, auth.clone());
        }
        if let Some(key) = &credential.api_key {
            builder = builder.header("x-api-key", key.clone());
        }

        let request = match body {
            Some(value) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string())),
            None => builder.body(Body::empty()),
        }
        .map_err(|e| format!("could not build request: {e}"))?;

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .map_err(|e| format!("dispatch failed: {e}"))?;

        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
            .await
            .map_err(|e| format!("could not read response: {e}"))?;

        Ok(ApiResponse {
            status,
            body: decode_body(&content_type, &bytes),
        })
    }

    /// Call a generated tool with the arguments a client supplied.
    pub async fn call_tool(
        &self,
        credential: &Credential,
        tool: &ApiTool,
        mut args: Map<String, Value>,
    ) -> Result<ApiResponse, String> {
        let mut path = tool.path.clone();
        for name in &tool.path_params {
            let value = args
                .remove(name)
                .filter(|v| !v.is_null())
                .ok_or_else(|| format!("missing required argument `{name}`"))?;
            path = path.replace(
                &format!("{{{name}}}"),
                &percent_encode(&scalar_to_string(&value)),
            );
        }

        let mut query: Vec<(String, String)> = Vec::new();
        for name in &tool.query_params {
            let Some(value) = args.remove(name) else {
                continue;
            };
            match value {
                Value::Null => {}
                // A repeated query key is how a list is sent; the API declares
                // no such parameter today, but a schema that grows one should
                // not need this file to change.
                Value::Array(items) => {
                    for item in items {
                        query.push((name.clone(), scalar_to_string(&item)));
                    }
                }
                other => query.push((name.clone(), scalar_to_string(&other))),
            }
        }
        if !query.is_empty() {
            let encoded: Vec<String> = query
                .iter()
                .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
                .collect();
            path.push('?');
            path.push_str(&encoded.join("&"));
        }

        let body = match tool.body {
            BodyMode::None => None,
            // Whatever the schema did not claim as a path or query parameter
            // is the body, so an argument the API does not know about still
            // reaches its validator and is reported there.
            BodyMode::Flat => Some(Value::Object(args)),
            BodyMode::Nested => Some(args.remove("body").unwrap_or_else(|| json!({}))),
        };

        self.send(credential, tool.method.clone(), &path, body.as_ref())
            .await
    }
}

/// JSON stays JSON. A server-sent event stream — the food search — is read
/// to its end and returned as the list of events it carried, since a model
/// cannot do anything with the wire framing. Anything else is text.
fn decode_body(content_type: &str, bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        return Value::Null;
    }
    if content_type.starts_with("application/json") {
        return serde_json::from_slice(bytes)
            .unwrap_or_else(|_| json!(String::from_utf8_lossy(bytes)));
    }
    if content_type.starts_with("text/event-stream") {
        return parse_sse(&String::from_utf8_lossy(bytes));
    }
    json!(String::from_utf8_lossy(bytes))
}

/// Split an SSE body into `{event, data}` objects, decoding JSON data where
/// it parses. Comment lines (the keep-alive pings) are dropped.
pub fn parse_sse(text: &str) -> Value {
    let mut events = Vec::new();
    let mut event_name: Option<String> = None;
    let mut data_lines: Vec<&str> = Vec::new();

    let flush =
        |event_name: &mut Option<String>, data_lines: &mut Vec<&str>, out: &mut Vec<Value>| {
            if data_lines.is_empty() && event_name.is_none() {
                return;
            }
            let data = data_lines.join("\n");
            let parsed = serde_json::from_str::<Value>(&data).unwrap_or(Value::String(data));
            out.push(json!({
                "event": event_name.take().unwrap_or_else(|| "message".into()),
                "data": parsed,
            }));
            data_lines.clear();
        };

    for line in text.lines() {
        if line.is_empty() {
            flush(&mut event_name, &mut data_lines, &mut events);
        } else if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
        // `id:`, `retry:` and comments carry nothing a caller needs.
    }
    flush(&mut event_name, &mut data_lines, &mut events);

    Value::Array(events)
}

fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Percent-encode everything outside the unreserved set. Small enough to
/// keep here rather than pull a crate in for one path segment.
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_events_become_a_list_with_decoded_data() {
        let body = "event: tier\ndata: {\"tier\":\"exact\",\"results\":[]}\n\n: keep-alive\n\nevent: done\ndata: {\"total\":0}\n\n";
        let parsed = parse_sse(body);
        assert_eq!(
            parsed,
            json!([
                {"event": "tier", "data": {"tier": "exact", "results": []}},
                {"event": "done", "data": {"total": 0}}
            ])
        );
    }

    #[test]
    fn percent_encoding_keeps_unreserved_bytes_only() {
        assert_eq!(percent_encode("chikn brest/2"), "chikn%20brest%2F2");
        assert_eq!(percent_encode("2026-01-15"), "2026-01-15");
    }

    #[test]
    fn a_json_body_is_parsed_and_text_is_kept() {
        assert_eq!(
            decode_body("application/json; charset=utf-8", br#"{"a":1}"#),
            json!({"a": 1})
        );
        assert_eq!(decode_body("text/plain", b"hello"), json!("hello"));
        assert_eq!(decode_body("application/json", b""), Value::Null);
    }
}
