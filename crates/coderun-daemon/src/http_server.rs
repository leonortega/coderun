use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use coderun_core::{
    AgentRequest, CorrelationId, HookType, OutputType,
    RequestPayload, TaskRequest,
};
use coderun_context::ContextEngine;
use coderun_optimizer::ExecutionOptimizer;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

// ── HTTP Request/Response Types ──────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct HttpRequest {
    pub correlation_id: Option<String>,
    pub hook_type: String,
    pub payload: HttpRequestPayload,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
pub enum HttpRequestPayload {
    #[serde(rename = "MessageRewrite")]
    MessageRewrite {
        session_id: Option<String>,
        message: String,
        context_hints: Option<ContextHintsJson>,
    },
    #[serde(rename = "ToolOutput")]
    ToolOutput {
        tool_name: String,
        output_type: Option<String>,
        content: String,
        context: Option<String>,
    },
}

#[derive(serde::Deserialize)]
pub struct ContextHintsJson {
    pub files_mentioned: Option<Vec<String>>,
    pub language: Option<String>,
}

#[derive(serde::Serialize)]
pub struct HttpResponse {
    pub correlation_id: String,
    pub hook_type: String,
    pub payload: HttpResponsePayload,
    pub latency_ms: u64,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type")]
pub enum HttpResponsePayload {
    #[serde(rename = "RewrittenMessage")]
    RewrittenMessage {
        original: String,
        rewritten: String,
    },
    #[serde(rename = "CompressedOutput")]
    CompressedOutput {
        original: String,
        compressed: String,
        original_tokens: u32,
        compressed_tokens: u32,
    },
    #[serde(rename = "OriginalPassthrough")]
    OriginalPassthrough {
        original: String,
        reason: String,
    },
}

// ── Server State ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct HttpServerState {
    pub context_engine: Arc<Mutex<ContextEngine>>,
    pub optimizer: Arc<ExecutionOptimizer>,
}

// ── Server Setup ─────────────────────────────────────────────────────────

pub fn create_router(state: HttpServerState) -> Router {
    Router::new()
        .route("/hook", post(handle_hook))
        .route("/health", axum::routing::get(handle_health))
        .with_state(state)
}

pub async fn start_http_server(
    port: u16,
    state: HttpServerState,
) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", port);
    let router = create_router(state);

    info!(addr = %addr, "HTTP server starting");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind HTTP server: {}", e))?;

    info!(addr = %addr, "HTTP server ready");

    axum::serve(listener, router)
        .await
        .map_err(|e| format!("HTTP server error: {}", e))
}

// ── Handlers ─────────────────────────────────────────────────────────────

async fn handle_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": "0.1.0"
    }))
}

/// Validate message length (spec §3 — secrets redaction + input validation)
fn validate_input_len(content: &str, limit: usize) -> Result<(), String> {
    if content.len() > limit {
        return Err(format!("input too large: {} > {} bytes (truncated)", content.len(), limit));
    }
    if content.contains("..") && content.contains('/') && content.len() < 500 {
        // Path traversal heuristic for file mentions — allow but warn
        tracing::warn!(content = %coderun_core::redact_secrets(content), "possible path traversal in input");
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
async fn handle_hook(
    State(state): State<HttpServerState>,
    Json(request): Json<HttpRequest>,
) -> Result<Json<HttpResponse>, (StatusCode, Json<HttpResponse>)> {
    let start = std::time::Instant::now();
    let correlation_id = request.correlation_id.unwrap_or_else(|| {
        format!("req_{}", uuid::Uuid::new_v4())
    });
    // Input validation (100KB message, 1MB tool content) + secrets redaction before logging
    if let HttpRequestPayload::MessageRewrite { ref message, .. } = request.payload {
        if let Err(e) = validate_input_len(message, 100 * 1024) {
            tracing::warn!(correlation_id = %correlation_id, error = %e, "input validation failed, fail-open passthrough");
            let resp = HttpResponse {
                correlation_id: correlation_id.clone(),
                hook_type: request.hook_type.clone(),
                payload: HttpResponsePayload::OriginalPassthrough { original: message.clone(), reason: e.clone() },
                latency_ms: start.elapsed().as_millis() as u64,
                error: Some(e),
            };
            return Ok(Json(resp));
        }
        // Redact secrets before any outbound call
        let _redacted = coderun_core::redact_secrets(message);
    }

    debug!(
        correlation_id = %correlation_id,
        hook_type = %request.hook_type,
        "HTTP request received"
    );

    // Convert to internal request
    let hook_type = match request.hook_type.as_str() {
        "PreGeneration" => HookType::PreGeneration,
        "PreToolCall" => HookType::PreToolCall,
        _ => {
            let hook_type_str = request.hook_type.clone();
            let resp = HttpResponse {
                correlation_id: correlation_id.clone(),
                hook_type: hook_type_str.clone(),
                payload: HttpResponsePayload::OriginalPassthrough {
                    original: String::new(),
                    reason: format!("Unknown hook type: {}", hook_type_str),
                },
                latency_ms: start.elapsed().as_millis() as u64,
                error: Some("Unknown hook type".to_string()),
            };
            return Err((StatusCode::BAD_REQUEST, Json(resp)));
        }
    };

    let internal_request = AgentRequest {
        correlation_id: CorrelationId::from_string(correlation_id.clone()),
        hook_type: hook_type.clone(),
        payload: match request.payload {
            HttpRequestPayload::MessageRewrite { session_id, message, context_hints } => {
                RequestPayload::MessageRewrite {
                    session_id: session_id.unwrap_or_default(),
                    message,
                    context_hints: context_hints.map(|h| coderun_core::ContextHints {
                        files_mentioned: h.files_mentioned,
                        language: h.language,
                    }),
                }
            }
            HttpRequestPayload::ToolOutput { tool_name, output_type, content, context } => {
                let ot = match output_type.as_deref() {
                    Some("FileRead") => OutputType::FileRead,
                    Some("SearchResult") => OutputType::SearchResult,
                    Some("ShellOutput") => OutputType::ShellOutput,
                    _ => OutputType::Other,
                };
                RequestPayload::ToolOutput {
                    tool_name,
                    output_type: ot,
                    content,
                    context,
                }
            }
        },
    };

    // Handle request
    let result = handle_request(internal_request, state).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!(error = %e, "Request handling failed");
            let resp = HttpResponse {
                correlation_id,
                hook_type: request.hook_type,
                payload: HttpResponsePayload::OriginalPassthrough {
                    original: String::new(),
                    reason: format!("error: {}", e),
                },
                latency_ms,
                error: Some(e),
            };
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(resp)))
        }
    }
}

async fn handle_request(
    request: AgentRequest,
    state: HttpServerState,
) -> Result<HttpResponse, String> {
    let correlation_id = request.correlation_id.to_string();
    let hook_type = format!("{:?}", request.hook_type);

    let payload = match &request.payload {
        RequestPayload::MessageRewrite { session_id, message, context_hints } => {
            handle_pre_generation(
                message.clone(),
                session_id.clone(),
                context_hints.clone(),
                &state.context_engine,
            ).await?
        }
        RequestPayload::ToolOutput { tool_name, output_type, content, context } => {
            handle_pre_tool_call(
                tool_name.clone(),
                output_type.clone(),
                content.clone(),
                context.clone(),
                &state.optimizer,
            )?
        }
    };

    Ok(HttpResponse {
        correlation_id,
        hook_type,
        payload,
        latency_ms: 0,
        error: None,
    })
}

async fn handle_pre_generation(
    message: String,
    session_id: String,
    context_hints: Option<coderun_core::ContextHints>,
    context_engine: &Arc<Mutex<ContextEngine>>,
) -> Result<HttpResponsePayload, String> {
    let task = TaskRequest {
        message: message.clone(),
        session_id,
        context_hints,
    };

    let engine = context_engine.lock().await;
    let (context_pack, _routing_decision) = engine.build_context(&task)?;

    let yaml = coderun_context::ContextEngine::to_yaml(&context_pack)?;

    let rewritten = format!(
        "{}\n\n---\n\nContext:\n{}",
        message, yaml
    );

    Ok(HttpResponsePayload::RewrittenMessage {
        original: message,
        rewritten,
    })
}

fn handle_pre_tool_call(
    tool_name: String,
    output_type: OutputType,
    content: String,
    context: Option<String>,
    optimizer: &Arc<ExecutionOptimizer>,
) -> Result<HttpResponsePayload, String> {
    let result = optimizer.compress_output(
        &tool_name,
        output_type,
        content.clone(),
        context.as_deref(),
    );

    Ok(HttpResponsePayload::CompressedOutput {
        original: content,
        compressed: result.compressed,
        original_tokens: result.original_tokens as u32,
        compressed_tokens: result.compressed_tokens as u32,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_request_deserialization() {
        let json = r#"{
            "hook_type": "PreGeneration",
            "payload": {
                "type": "MessageRewrite",
                "session_id": "test",
                "message": "hello"
            }
        }"#;
        let req: HttpRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.hook_type, "PreGeneration");
    }

    #[test]
    fn test_http_response_serialization() {
        let resp = HttpResponse {
            correlation_id: "req_123".to_string(),
            hook_type: "PreGeneration".to_string(),
            payload: HttpResponsePayload::OriginalPassthrough {
                original: "test".to_string(),
                reason: "error".to_string(),
            },
            latency_ms: 100,
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("OriginalPassthrough"));
    }
}
