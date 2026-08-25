use std::time::Duration;

use coderun_core::{traits::IWorkflowEngine, Config, TaskRequest};
use coderun_core::error::{CoderunError, Result};
use tracing::{warn};

/// DBOS Transact sidecar engine — HTTP bridge to Node process (v0.6.0 native async)
/// Required since v0.6.0 (SQLite + Litestream); no `block_on_in_thread` hack.
pub struct DBOSWorkflowEngine {
    endpoint: String,
    shared_secret: Option<String>,
    client: reqwest::Client,
}

impl DBOSWorkflowEngine {
    pub fn new(endpoint: String, shared_secret: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(5000))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { endpoint, shared_secret, client }
    }

    fn hmac_header(&self, body: &str) -> Option<String> {
        let secret = self.shared_secret.as_ref()?;
        Some(coderun_core::secrets::hmac_hex(secret, body))
    }
}

#[async_trait::async_trait]
impl IWorkflowEngine for DBOSWorkflowEngine {
    async fn start_workflow(&self, task: &TaskRequest, _config: &Config) -> Result<String> {
        let endpoint = self.endpoint.clone();
        let body = serde_json::json!({
            "task": task.message,
            "session_id": task.session_id,
            "require_approval": false,
            "workflow_id": format!("wf_{}", uuid::Uuid::new_v4()),
        });
        let body_str = body.to_string();
        let hmac = self.hmac_header(&body_str);
        let workflow_id = body["workflow_id"].as_str().unwrap_or("wf_local").to_string();

        let mut req = self.client
            .post(format!("{}/workflow/start", endpoint))
            .header("Content-Type", "application/json")
            .body(body_str.clone());
        if let Some(sig) = hmac { req = req.header("X-Coderun-Signature", sig); }

        match tokio::time::timeout(Duration::from_millis(5000), req.send()).await {
            Ok(Ok(resp)) if resp.status().is_success() => Ok(workflow_id),
            Ok(Ok(resp)) => {
                warn!(status = %resp.status(), "DBOS start failed");
                Err(CoderunError::InvalidRequest(format!("DBOS start failed: {}", resp.status())))
            }
            Ok(Err(e)) => {
                warn!(error = %e, "DBOS request error");
                Err(CoderunError::InvalidRequest(format!("DBOS request error: {}", e)))
            }
            Err(_) => {
                warn!("DBOS start timeout (5s)");
                Err(CoderunError::Timeout("DBOS start timeout".to_string()))
            }
        }
    }

    async fn get_status(&self, workflow_id: &str) -> Result<String> {
        let endpoint = self.endpoint.clone();
        let wid = workflow_id.to_string();
        let resp = tokio::time::timeout(
            Duration::from_millis(3000),
            self.client.get(format!("{}/workflow/{}", endpoint, wid)).send(),
        ).await;
        match resp {
            Ok(Ok(r)) if r.status().is_success() => {
                let text = r.text().await.unwrap_or_else(|_| format!("{{\"workflow_id\":\"{}\",\"status\":\"running\"}}", workflow_id));
                Ok(text)
            }
            Ok(Ok(r)) => Err(CoderunError::InvalidRequest(format!("Workflow {} status {} ", workflow_id, r.status()))),
            Ok(Err(e)) => Err(CoderunError::InvalidRequest(format!("Workflow {} error {}", workflow_id, e))),
            Err(_) => Err(CoderunError::Timeout(format!("Workflow {} timeout", workflow_id))),
        }
    }

    async fn is_available(&self) -> bool {
        let resp = tokio::time::timeout(
            Duration::from_millis(1000),
            self.client.get(format!("{}/health", self.endpoint)).send(),
        ).await;
        matches!(resp, Ok(Ok(r)) if r.status().is_success())
    }
}

pub use coderun_core::secrets::verify_hmac;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hmac_verify_core() {
        let secret = "s3cret";
        let body = r#"{"task":"hi"}"#;
        let sig = coderun_core::secrets::hmac_hex(secret, body);
        assert!(verify_hmac(secret, body, &sig));
        assert!(!verify_hmac(secret, body, "bad"));
        assert!(!verify_hmac("other", body, &sig));
        // empty body
        let empty_sig = coderun_core::secrets::hmac_hex(secret, "");
        assert!(verify_hmac(secret, "", &empty_sig));
        assert!(!verify_hmac(secret, "x", &empty_sig));
    }

    #[test]
    fn test_hmac_header_none_without_secret() {
        let engine = DBOSWorkflowEngine::new("http://localhost:0".to_string(), None);
        assert!(engine.hmac_header(r#"{"x":1}"#).is_none());
    }

    #[test]
    fn test_hmac_header_with_secret() {
        let engine = DBOSWorkflowEngine::new("http://localhost:0".to_string(), Some("my-secret".to_string()));
        let body = r#"{"task":"hello"}"#;
        let hdr = engine.hmac_header(body).unwrap();
        assert_eq!(hdr.len(), 64);
        assert!(verify_hmac("my-secret", body, &hdr));
        assert!(!verify_hmac("wrong", body, &hdr));
    }

    #[tokio::test]
    async fn test_dbos_not_available_without_sidecar() {
        let engine = DBOSWorkflowEngine::new("http://localhost:59999".to_string(), None);
        assert!(!engine.is_available().await);
    }

    #[tokio::test]
    async fn test_workflow_start_fails_when_dbos_down_required() {
        let engine = DBOSWorkflowEngine::new("http://localhost:59999".to_string(), None);
        let task = TaskRequest { message: "refactor auth".to_string(), session_id: "s1".to_string(), context_hints: None };
        let config = Config::default();
        // v0.6.0: fail-closed when DBOS required
        assert!(engine.start_workflow(&task, &config).await.is_err());
    }

    #[tokio::test]
    async fn test_workflow_start_with_mock_server() {
        let app = axum::Router::new()
            .route("/workflow/start", axum::routing::post(|| async { axum::Json(serde_json::json!({"workflow_id":"wf_mock"})) }))
            .route("/health", axum::routing::get(|| async { "ok" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        tokio::time::sleep(Duration::from_millis(200)).await;
        let engine = DBOSWorkflowEngine::new(format!("http://{}", addr), None);
        assert!(engine.is_available().await);
        let task = TaskRequest { message: "hi".to_string(), session_id: "s".to_string(), context_hints: None };
        let id = engine.start_workflow(&task, &Config::default()).await.unwrap();
        assert!(id.starts_with("wf_"));
    }
}
