//! Model Context Protocol server mode.
//!
//! `ascii-forge mcp` speaks MCP over stdio using `rmcp`. Every artifact lives
//! inside the project directory, stdout carries protocol messages only, and
//! normal CLI commands never depend on this module.

mod create;
mod error;
mod export;
mod params;
mod paths;
mod preview;

use anyhow::{Context, Result};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ErrorData};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

use error::ToolError;
use params::{CreateSceneParams, ExportParams, RenderPreviewParams};

/// Stateless, tools-only MCP server.
#[derive(Debug, Clone, Copy, Default)]
pub struct AsciiForgeServer;

/// Start the stdio MCP server and block until the client disconnects.
pub fn run_stdio() -> Result<()> {
    init_stderr_tracing();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("could not build the Tokio runtime for the MCP server")?;

    runtime.block_on(async {
        let service = AsciiForgeServer
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|error| anyhow::anyhow!("MCP server failed to start: {error}"))?;
        service
            .waiting()
            .await
            .map_err(|error| anyhow::anyhow!("MCP server task failed: {error}"))?;
        Ok(())
    })
}

/// Diagnostics go to stderr so stdout stays a pure JSON-RPC channel.
fn init_stderr_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false);
    let _ = subscriber.try_init();
}

/// Run blocking work off the async runtime.
async fn run_blocking<T, F>(
    label: &'static str,
    task: F,
) -> std::result::Result<std::result::Result<T, ToolError>, ErrorData>
where
    T: Send + 'static,
    F: FnOnce() -> std::result::Result<T, ToolError> + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|error| ErrorData::internal_error(format!("{label} task failed: {error}"), None))
}

fn finish(outcome: std::result::Result<Value, ToolError>) -> CallToolResult {
    outcome.map_or_else(ToolError::into_result, CallToolResult::structured)
}

fn config_payload(
    build: impl FnOnce() -> anyhow::Result<Value>,
    description: &str,
) -> std::result::Result<Value, ToolError> {
    build().map_err(|error| ToolError::config(format!("could not build {description}: {error:#}")))
}

#[tool_router]
impl AsciiForgeServer {
    /// Create a scene from an offline template or an AI prompt.
    #[tool(
        name = "create_scene",
        description = "Create an ASCII Forge scene from one offline template \
                       (logo, promo, glitch, matrix, lower_third) or from an AI \
                       prompt, then write pretty scene JSON inside the project. \
                       Returns the relative scene path plus origin, frames, seed, \
                       attempts, cache and token usage metadata."
    )]
    async fn create_scene(
        &self,
        Parameters(params): Parameters<CreateSceneParams>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let outcome = run_blocking("create_scene", move || create::create_scene(params)).await?;
        Ok(finish(outcome))
    }

    /// Render a bounded text or ANSI preview of one scene frame.
    #[tool(
        name = "render_preview",
        description = "Validate a project-relative scene and return a bounded text \
                       or ANSI preview of one frame. Rows and columns are capped by \
                       max_cols/max_rows, and the result reports the frame count and \
                       whether the preview was truncated.",
        annotations(read_only_hint = true)
    )]
    async fn render_preview(
        &self,
        Parameters(params): Parameters<RenderPreviewParams>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let outcome =
            run_blocking("render_preview", move || preview::render_preview(params)).await?;
        Ok(finish(outcome))
    }

    /// Export a scene as PNG, JPEG, SVG, GIF, MP4, WebM, ANSI or text.
    #[tool(
        name = "export",
        description = "Validate a project-relative scene and write an artifact under \
                       .ascii-forge/artifacts/. PNG, JPEG, SVG, ANSI and text use one \
                       frame; GIF, MP4 and WebM compose the whole timeline. Returns \
                       the relative path, MIME type, format, frame count and byte size.",
        annotations(read_only_hint = false)
    )]
    async fn export(
        &self,
        Parameters(params): Parameters<ExportParams>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let outcome = run_blocking("export", move || export::export_scene(params)).await?;
        Ok(finish(outcome))
    }

    /// List FIGlet fonts and the resolved raster font.
    #[tool(
        name = "list_fonts",
        description = "Return the FIGlet font catalog and the resolved monospace \
                       render font used by raster exports.",
        annotations(read_only_hint = true)
    )]
    async fn list_fonts(&self) -> std::result::Result<CallToolResult, ErrorData> {
        let outcome = run_blocking("list_fonts", || {
            config_payload(crate::cli::fonts_json, "the font catalog")
        })
        .await?;
        Ok(finish(outcome))
    }

    /// Report FFmpeg, encoder, font and config-directory diagnostics.
    #[tool(
        name = "doctor",
        description = "Return the structured diagnostics report: FFmpeg, FFprobe, \
                       libx264, libvpx-vp9, the resolved font and the config directory.",
        annotations(read_only_hint = true)
    )]
    async fn doctor(&self) -> std::result::Result<CallToolResult, ErrorData> {
        let outcome = run_blocking("doctor", || {
            config_payload(crate::cli::doctor_json, "diagnostics")
        })
        .await?;
        Ok(finish(outcome))
    }
}

/// Advertise this crate, not `rmcp`, as the server implementation.
///
/// `#[tool_router(server_handler)]` emits a handler whose name and version come
/// from rmcp's own build environment, so the `ServerHandler` impl is declared
/// explicitly here to keep `serverInfo` tied to ASCII Forge.
#[tool_handler(router = Self::tool_router(), name = "ascii-forge")]
impl ServerHandler for AsciiForgeServer {}
