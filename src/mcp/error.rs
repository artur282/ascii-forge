//! Structured tool errors.
//!
//! MCP tools never abort the server: every failure becomes a
//! [`CallToolResult::structured_error`] with a stable machine-readable code.

use rmcp::model::CallToolResult;
use serde_json::json;

/// A tool-level failure with a stable code and a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    code: &'static str,
    message: String,
}

impl ToolError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new("invalid_input", message)
    }

    pub fn unsafe_path(message: impl Into<String>) -> Self {
        Self::new("unsafe_path", message)
    }

    pub fn scene_not_found(message: impl Into<String>) -> Self {
        Self::new("scene_not_found", message)
    }

    pub fn invalid_scene(message: impl Into<String>) -> Self {
        Self::new("invalid_scene", message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new("io_error", message)
    }

    pub fn output_exists(message: impl Into<String>) -> Self {
        Self::new("output_exists", message)
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::new("config_error", message)
    }

    pub fn provider(message: impl Into<String>) -> Self {
        Self::new("provider_error", message)
    }

    pub fn generation(message: impl Into<String>) -> Self {
        Self::new("generation_failed", message)
    }

    pub fn render(message: impl Into<String>) -> Self {
        Self::new("render_failed", message)
    }

    pub fn export(message: impl Into<String>) -> Self {
        Self::new("export_failed", message)
    }

    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// The MCP tool result for this failure.
    pub fn into_result(self) -> CallToolResult {
        let code = self.code();
        CallToolResult::structured_error(json!({
            "code": code,
            "message": self.message,
        }))
    }
}

/// Remove a process-held secret from an error message before it reaches a client.
pub fn redact_secret(message: impl Into<String>, secret: &str) -> String {
    let message = message.into();
    let secret = secret.trim();
    if secret.is_empty() {
        message
    } else {
        message.replace(secret, "<redacted>")
    }
}
