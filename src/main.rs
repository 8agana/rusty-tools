use anyhow::Result;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParam, CallToolResult, InitializeRequestParam, InitializeResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParam, Resource, ServerCapabilities,
        ServerInfo, Tool,
    },
    service::{RequestContext, RoleServer},
    transport::stdio,
};
use serde_json::json;
use std::borrow::Cow;
use std::future::Future;

use std::process::Command as StdCommand;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

#[derive(Clone)]
struct RustyToolsServer;

impl ServerHandler for RustyToolsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Rust development tools for formatting, linting, and analysis".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<InitializeResult, McpError>> + Send + '_ {
        async move { Ok(self.get_info()) }
    }

    #[allow(clippy::manual_async_fn)]
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            let tools = vec![
                Tool::new(
                    Cow::Borrowed("cargo_fmt"),
                    Cow::Borrowed("Format Rust code using rustfmt"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code to format"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_clippy"),
                    Cow::Borrowed("Analyze code with clippy for improvements"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code to analyze"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_check"),
                    Cow::Borrowed("Type-check Rust code without building"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code to check"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("rustc_explain"),
                    Cow::Borrowed("Explain a Rust compiler error code"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "error_code": {"type": "string", "description": "Error code like E0308"}
                        },
                        "required": ["error_code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_fix"),
                    Cow::Borrowed("Automatically fix compiler warnings"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code to fix"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_audit"),
                    Cow::Borrowed("Scan for security vulnerabilities in dependencies"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code with Cargo.toml to audit"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_test"),
                    Cow::Borrowed("Run tests on Rust code"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code with tests to run"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_build"),
                    Cow::Borrowed("Build Rust code (produces artifacts)"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code to build-check"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_search"),
                    Cow::Borrowed("Search crates.io for packages"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string", "description": "Search query for crates.io"}
                        },
                        "required": ["query"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_tree"),
                    Cow::Borrowed("Show dependency tree for Rust code"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code with dependencies to analyze"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_doc"),
                    Cow::Borrowed("Generate documentation for Rust code"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code to generate documentation for"}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("rust_analyzer"),
                    Cow::Borrowed(
                        "Analyze Rust code with rust-analyzer for diagnostics and suggestions",
                    ),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code to analyze"}
                        },
                        "required": ["code"]
                    })),
                ),
            ];

            Ok(ListToolsResult {
                tools,
                ..Default::default()
            })
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        async move {
            // Return an empty resources list; this satisfies clients that probe resources/list.
            Ok(ListResourcesResult {
                resources: Vec::<Resource>::new(),
                ..Default::default()
            })
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            match request.name.as_ref() {
                "cargo_fmt" => {
                    let code = get_code_arg(&request, "cargo_fmt")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["fmt", "--", "--emit=stdout"], None).await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "cargo_clippy" => {
                    let code = get_code_arg(&request, "cargo_clippy")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["clippy", "--", "-W", "clippy::all"],
                        Some(Duration::from_secs(30)),
                    )
                    .await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "cargo_check" => {
                    let code = get_code_arg(&request, "cargo_check")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["check"], Some(Duration::from_secs(30))).await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "rustc_explain" => {
                    let error_code = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("error_code"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::invalid_params("error_code parameter required", None)
                        })?;

                    let output = StdCommand::new("rustc")
                        .args(["--explain", error_code])
                        .output()
                        .map_err(|e| {
                            McpError::internal_error(format!("Failed to run rustc: {}", e), None)
                        })?;

                    let explanation = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    if !output.status.success() && !stderr.is_empty() {
                        return Ok(CallToolResult::structured(json!({
                            "explanation": format!("Error: {}", stderr.trim()),
                            "success": false
                        })));
                    }

                    Ok(CallToolResult::structured(json!({
                        "status": 0,
                        "success": true,
                        "stdout": explanation.trim(),
                        "stderr": "",
                        "duration_ms": 0
                    })))
                }
                "cargo_fix" => {
                    let code = get_code_arg(&request, "cargo_fix")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["fix", "--allow-dirty"],
                        Some(Duration::from_secs(60)),
                    )
                    .await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "cargo_audit" => {
                    let code = get_code_arg(&request, "cargo_audit")?;

                    // Check if cargo-audit is installed
                    if which::which("cargo-audit").is_err() {
                        return Ok(CallToolResult::structured(json!({
                            "error": "cargo-audit not installed. Install with: cargo install cargo-audit",
                            "success": false
                        })));
                    }

                    let result =
                        run_rust_tool(code, &["audit"], Some(Duration::from_secs(30))).await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "cargo_test" => {
                    let code = get_code_arg(&request, "cargo_test")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["test", "--", "--nocapture"],
                        Some(Duration::from_secs(60)),
                    )
                    .await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "cargo_build" => {
                    let code = get_code_arg(&request, "cargo_build")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["build", "--message-format=short"],
                        Some(Duration::from_secs(45)),
                    )
                    .await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "cargo_search" => {
                    let query = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("query"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::invalid_params("query parameter required", None)
                        })?;

                    let output = StdCommand::new("cargo")
                        .args(["search", query, "--limit", "10"])
                        .output()
                        .map_err(|e| {
                            McpError::internal_error(
                                format!("Failed to run cargo search: {}", e),
                                None,
                            )
                        })?;

                    let results = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    Ok(CallToolResult::structured(json!({
                        "status": output.status.code().unwrap_or(-1),
                        "success": output.status.success(),
                        "stdout": results.trim(),
                        "stderr": stderr.trim(),
                        "duration_ms": 0
                    })))
                }
                "cargo_tree" => {
                    let code = get_code_arg(&request, "cargo_tree")?;
                    let result =
                        run_rust_tool(code, &["tree"], Some(Duration::from_secs(30))).await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "cargo_doc" => {
                    let code = get_code_arg(&request, "cargo_doc")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["doc", "--no-deps"], Some(Duration::from_secs(60)))
                            .await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                "rust_analyzer" => {
                    let code = get_code_arg(&request, "rust_analyzer")?;
                    validate_rust_code(code)?;

                    // Check if rust-analyzer is installed
                    if which::which("rust-analyzer").is_err() {
                        return Ok(CallToolResult::structured(json!({
                            "error": "rust-analyzer not installed. Install via rustup or package manager",
                            "success": false
                        })));
                    }

                    // For now, just run cargo check as rust-analyzer LSP integration is complex
                    let result = run_rust_tool(
                        code,
                        &["check", "--message-format=json"],
                        Some(Duration::from_secs(30)),
                    )
                    .await?;
                    Ok(CallToolResult::structured(json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    })))
                }
                _ => Err(McpError::method_not_found::<
                    rmcp::model::CallToolRequestMethod,
                >()),
            }
        }
    }
}

fn get_code_arg<'a>(
    request: &'a CallToolRequestParam,
    tool_name: &str,
) -> Result<&'a str, McpError> {
    request
        .arguments
        .as_ref()
        .and_then(|args| args.get("code"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            McpError::invalid_params(format!("code parameter required for {}", tool_name), None)
        })
}

fn validate_rust_code(code: &str) -> Result<(), McpError> {
    if code.trim().is_empty() {
        return Err(McpError::invalid_params("Code cannot be empty", None));
    }

    // Basic validation - check for potentially dangerous operations
    let dangerous_patterns = ["std::process::Command", "std::fs::", "std::net::", "unsafe"];
    for pattern in &dangerous_patterns {
        if code.contains(pattern) {
            return Err(McpError::invalid_params(
                format!("Code contains potentially unsafe pattern: {}", pattern),
                None,
            ));
        }
    }

    Ok(())
}

struct ExecResult {
    stdout: String,
    stderr: String,
    status: i32,
    duration_ms: u128,
}

async fn run_rust_tool(
    code: &str,
    args: &[&str],
    timeout: Option<Duration>,
) -> Result<ExecResult, McpError> {
    // Create a temporary directory for the Rust project
    let temp_dir = tempfile::tempdir()
        .map_err(|e| McpError::internal_error(format!("Failed to create temp dir: {}", e), None))?;

    let project_path = temp_dir.path();

    // Initialize a new Cargo project
    let output = StdCommand::new("cargo")
        .args(["init", "--name", "temp_project"])
        .current_dir(project_path)
        .output()
        .map_err(|e| McpError::internal_error(format!("Failed to run cargo init: {}", e), None))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(McpError::internal_error(
            format!("Cargo init failed: {}", stderr),
            None,
        ));
    }

    // Write the provided code to src/main.rs
    let main_rs_path = project_path.join("src").join("main.rs");
    std::fs::write(&main_rs_path, code)
        .map_err(|e| McpError::internal_error(format!("Failed to write code: {}", e), None))?;

    // Run the specified cargo command
    let start = Instant::now();
    let mut cmd = Command::new("cargo");
    cmd.args(args)
        .current_dir(project_path)
        .env("CARGO_TERM_COLOR", "never");

    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| McpError::internal_error(format!("Failed to spawn cargo: {}", e), None))?;

    let mut stdout_reader = child
        .stdout
        .take()
        .ok_or_else(|| McpError::internal_error("Failed to capture stdout", None))?;
    let mut stderr_reader = child
        .stderr
        .take()
        .ok_or_else(|| McpError::internal_error("Failed to capture stderr", None))?;

    let out_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stdout_reader.read_to_end(&mut buf).await;
        buf
    });
    let err_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr_reader.read_to_end(&mut buf).await;
        buf
    });

    let status = if let Some(dur) = timeout {
        match tokio::time::timeout(dur, child.wait()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                return Err(McpError::internal_error(
                    format!("Failed to wait for cargo: {}", e),
                    None,
                ));
            }
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(McpError::internal_error(
                    "Command timed out".to_string(),
                    None,
                ));
            }
        }
    } else {
        child.wait().await.map_err(|e| {
            McpError::internal_error(format!("Failed to wait for cargo: {}", e), None)
        })?
    };

    let duration_ms = start.elapsed().as_millis();

    let stdout_bytes = out_handle
        .await
        .map_err(|e| McpError::internal_error(format!("Stdout task failed: {}", e), None))?;
    let stderr_bytes = err_handle
        .await
        .map_err(|e| McpError::internal_error(format!("Stderr task failed: {}", e), None))?;

    let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
    let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
    let status = status.code().unwrap_or(-1);

    Ok(ExecResult {
        stdout,
        stderr,
        status,
        duration_ms,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    // Log server start to stderr (won't interfere with MCP protocol)
    eprintln!("Rusty Tools MCP Server starting...");

    let handler = RustyToolsServer;
    let service = handler
        .serve(stdio())
        .await
        .map_err(|e| anyhow::anyhow!("failed to start server: {}", e))?;

    service.waiting().await?;

    eprintln!("Rusty Tools MCP Server shutting down");
    Ok(())
}
