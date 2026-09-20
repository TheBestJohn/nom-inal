//! MCP, served by the API itself.
//!
//! `/mcp` speaks the Model Context Protocol over Streamable HTTP, in the same
//! process and behind the same API keys as everything else. There is no
//! second server to run, no separate credential, and nothing to install
//! when the API grows: the tools are read from the OpenAPI registry at
//! startup (`registry`) and executed by dispatching an HTTP request to the
//! router in-process (`dispatch`), so an endpoint that is documented is a
//! tool that works, with the API's own validation and errors.
//!
//! On top of that sit a few composite tools shaped like sentences
//! (`composite`) and a resource explaining the domain (`guide.md`).
//!
//! The transport runs stateless: every POST is a self-contained JSON-RPC
//! exchange carrying its own `Authorization` header, which is what a key-
//! authenticated service wants — there is no session to hijack, and a
//! client that reconnects loses nothing.

pub mod composite;
pub mod dispatch;
pub mod registry;

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{request::Parts, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
    ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{json, Value};
use utoipa::OpenApi;

use crate::auth::{self, Credential as AuthCredential, CurrentUser};
use crate::error::ErrorBody;
use crate::state::AppState;

use dispatch::{ApiResponse, Credential, Dispatcher};
use registry::ApiTool;

/// The one resource: a description of the domain, so a model reads the rules
/// instead of inferring them from field names.
pub const GUIDE_URI: &str = "nom-inal://guide";
const GUIDE: &str = include_str!("guide.md");

/// The MCP server. Cheap to clone; the transport asks for one per request.
#[derive(Clone)]
pub struct McpServer {
    tools: Arc<Vec<ApiTool>>,
    index: Arc<HashMap<String, usize>>,
    dispatcher: Dispatcher,
}

impl McpServer {
    /// Read the registry and bind it to the router the tools will call.
    pub fn new(api: Router) -> Self {
        let doc = serde_json::to_value(crate::openapi::ApiDoc::openapi())
            .expect("the OpenAPI document serialises");
        let tools = registry::tools_from_openapi(&doc);
        tracing::info!(
            generated = tools.len(),
            composite = composite::TOOLS.len(),
            "MCP tools assembled from the OpenAPI registry"
        );
        tracing::debug!(
            tools = ?tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            "generated MCP tools"
        );
        let index = registry::index(&tools);
        Self {
            tools: Arc::new(tools),
            index: Arc::new(index),
            dispatcher: Dispatcher::new(api),
        }
    }

    /// Who is calling, as established by the middleware on `/mcp`.
    fn caller(ctx: &RequestContext<RoleServer>) -> Result<(CurrentUser, Credential), McpError> {
        let parts = ctx.extensions.get::<Parts>().ok_or_else(|| {
            McpError::internal_error("request parts were not attached to the MCP request", None)
        })?;
        let user = parts
            .extensions
            .get::<CurrentUser>()
            .copied()
            .ok_or_else(|| {
                McpError::invalid_request("no API key was authenticated for this request", None)
            })?;
        Ok((user, Credential::from_headers(&parts.headers)))
    }

    /// Every tool the caller may use. A read key does not see the tools it
    /// could not call, rather than being offered them and refused.
    fn tools_for(&self, user: &CurrentUser) -> Vec<Tool> {
        let mut out: Vec<Tool> = composite::TOOLS
            .iter()
            .filter(|t| user.can_write || !t.writes)
            .map(|t| {
                let schema = (t.input_schema)().as_object().cloned().unwrap_or_default();
                Tool::new(t.name, t.description, schema).with_annotations(
                    ToolAnnotations::default()
                        .read_only(!t.writes)
                        .destructive(false)
                        .open_world(false),
                )
            })
            .collect();

        out.extend(
            self.tools
                .iter()
                .filter(|t| user.can_write || !t.writes())
                .map(|t| {
                    let annotations = ToolAnnotations::default()
                        .read_only(!t.writes())
                        .destructive(t.method == axum::http::Method::DELETE)
                        .idempotent(t.method != axum::http::Method::POST)
                        .open_world(false);
                    Tool::new(
                        t.name.clone(),
                        t.description.clone(),
                        t.input_schema.clone(),
                    )
                    .with_annotations(annotations)
                }),
        );
        out
    }
}

/// An API response as a tool result. Anything the API refused is a tool
/// error carrying the API's own `{error, message}`, so the model reads the
/// same explanation a person would.
///
/// `structuredContent` has to be a JSON object, so a list or a string comes
/// back as text only — still JSON, still the API's exact shape, just not
/// wrapped in a key the API never had.
fn tool_result(response: ApiResponse) -> CallToolResult {
    let ApiResponse { status, body } = response;
    let body = match body {
        // A 204 has no body; say what happened rather than nothing.
        Value::Null => json!({ "ok": status.is_success(), "status": status.as_u16() }),
        other => other,
    };
    if status.is_success() {
        match body {
            Value::Object(_) => CallToolResult::structured(body),
            other => CallToolResult::success(vec![ContentBlock::text(other.to_string())]),
        }
    } else {
        CallToolResult::structured_error(match body {
            Value::Object(mut obj) => {
                obj.entry("status")
                    .or_insert_with(|| json!(status.as_u16()));
                Value::Object(obj)
            }
            // A rejection that never went through `ApiError` — axum's own
            // path-parameter message, say — is plain text.
            other => json!({
                "error": "bad_request",
                "status": status.as_u16(),
                "message": other,
            }),
        })
    }
}

fn refused(message: &str) -> CallToolResult {
    CallToolResult::structured_error(json!({ "error": "forbidden", "message": message }))
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("nom-inal", env!("CARGO_PKG_VERSION"))
                .with_title("nom-inal nutrition tracker"),
        )
        .with_instructions(
            "nom-inal tracks weight, food, recipes and daily targets for one person. \
             Read the resource nom-inal://guide first: it explains that nutrients are per 100 g, \
             that a diary entry is a food in grams or a recipe in servings, and how goals differ \
             from budgets. Use `log_food` to log what someone ate (it reports what it would log; \
             pass confirm: true to write), `today` for the day, `progress` for a window, and \
             `add_recipe_from_text` to build a recipe from ingredient lines. Every other tool is \
             one API operation, named `{tag}_{operation}`; a new endpoint appears here on its own.",
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let (user, _) = Self::caller(&ctx)?;
        Ok(ListToolsResult::with_all_items(self.tools_for(&user)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let (user, credential) = Self::caller(&ctx)?;
        let args = request.arguments.unwrap_or_default();
        let name = request.name.as_ref();

        if let Some(composite) = composite::TOOLS.iter().find(|t| t.name == name) {
            if composite.writes && !user.can_write {
                return Ok(refused(
                    "this API key is read-only; create a key with the 'write' scope to make changes",
                )
                .into());
            }
            let outcome = composite::call(name, &self.dispatcher, &credential, &user, args)
                .await
                .ok_or_else(|| McpError::internal_error("composite tool vanished", None))?;
            return Ok(match outcome {
                Ok(value) => CallToolResult::structured(value),
                Err(composite::ToolError(body)) => CallToolResult::structured_error(body),
            }
            .into());
        }

        let Some(tool) = self.index.get(name).map(|i| &self.tools[*i]) else {
            return Err(McpError::invalid_params(
                format!("unknown tool `{name}`"),
                None,
            ));
        };
        // The extractor would refuse this anyway; answering here keeps the
        // message the same as tools/list, which did not offer the tool.
        if tool.writes() && !user.can_write {
            return Ok(refused(
                "this API key is read-only; create a key with the 'write' scope to make changes",
            )
            .into());
        }

        match self.dispatcher.call_tool(&credential, tool, args).await {
            Ok(response) => Ok(tool_result(response).into()),
            Err(message) => Ok(CallToolResult::structured_error(json!({
                "error": "bad_request",
                "message": message,
            }))
            .into()),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(vec![Resource::new(
            GUIDE_URI, "guide",
        )
        .with_title("How nom-inal's numbers work")
        .with_description(
            "Nutrient basis, diary entries, recipes and free-text ingredients, goals versus \
             budgets, dates and meals. Read before logging anything.",
        )
        .with_mime_type("text/markdown")]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        if request.uri != GUIDE_URI {
            return Err(McpError::resource_not_found(
                format!("no resource at {}", request.uri),
                None,
            ));
        }
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(GUIDE, GUIDE_URI).with_mime_type("text/markdown")
        ])
        .into())
    }
}

/// Authenticate the MCP request with the API's own key resolution and hand
/// the caller to the handler.
///
/// A missing or unknown key is refused here, with the API's usual 401, before
/// any JSON-RPC is parsed. A session token is refused too: `/mcp` is for
/// keys, which can be scoped and revoked one at a time, and a leaked browser
/// session should not double as an assistant's credential.
async fn require_api_key(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    match auth::authenticate_headers(&state, request.headers()).await {
        Ok(user) if user.credential == AuthCredential::ApiKey => {
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Ok(_) => (
            StatusCode::UNAUTHORIZED,
            axum::Json(ErrorBody {
                error: "unauthorized".into(),
                message:
                    "the MCP endpoint accepts API keys only; create one under Settings → API keys"
                        .into(),
            }),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

/// The `/mcp` route, to be merged into the application router.
///
/// `api` is the router the tools dispatch to. It must be the same one the
/// network serves, minus the outermost layers, so that a tool call and an
/// HTTP call are indistinguishable to the handlers.
pub fn router(state: AppState, api: Router) -> Router {
    let server = McpServer::new(api);

    let config = StreamableHttpServerConfig::default()
        // One POST, one answer. Sessions would only add state to lose.
        .with_legacy_session_mode(false)
        // Plain JSON when the answer is a single message, which it always is
        // here; the SSE framing is kept for clients that stream.
        .with_json_response(true)
        // The transport's default only admits loopback hosts, a guard for
        // servers that run unauthenticated on a laptop. This one sits behind
        // a reverse proxy under whatever name the instance has, and every
        // request has already presented a key by the time it gets here.
        .disable_allowed_hosts();

    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );

    Router::new()
        .route_service("/mcp", service)
        .layer(middleware::from_fn_with_state(state, require_api_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_and_generated_tool_names_do_not_collide() {
        let doc = serde_json::to_value(crate::openapi::ApiDoc::openapi()).unwrap();
        let generated = registry::tools_from_openapi(&doc);
        for composite in composite::TOOLS {
            assert!(
                !generated.iter().any(|t| t.name == composite.name),
                "{} is both generated and hand-written",
                composite.name
            );
        }
    }

    #[test]
    fn the_guide_covers_what_it_promises() {
        for topic in [
            "per 100 g",
            "servings",
            "label",
            "budget",
            "goal",
            "logged_on",
            "untracked_count",
            "meta_health",
        ] {
            assert!(GUIDE.contains(topic), "guide does not mention {topic}");
        }
    }
}
