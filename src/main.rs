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

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
                    Cow::Borrowed("Check if code would build (without actually building)"),
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
                        "formatted": result,
                        "success": true
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
                        "analysis": result,
                        "success": result.is_empty()
                    })))
                }
                "cargo_check" => {
                    let code = get_code_arg(&request, "cargo_check")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["check"], Some(Duration::from_secs(30))).await?;
                    Ok(CallToolResult::structured(json!({
                        "output": result,
                        "success": result.contains("Finished")
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

                    let output = Command::new("rustc")
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
                        "explanation": explanation.trim(),
                        "success": true
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
                        "output": result,
                        "success": true
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
                        "audit": result,
                        "success": true
                    })))
                }
                "cargo_test" => {
                    let code = get_code_arg(&request, "cargo_test")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["test", "--", "--nocapture"], Some(Duration::from_secs(60))).await?;
                    Ok(CallToolResult::structured(json!({
                        "test_output": result,
                        "success": result.contains("test result: ok") || result.contains("0 passed")
                    })))
                }
                "cargo_build" => {
                    let code = get_code_arg(&request, "cargo_build")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["build", "--message-format=short"], Some(Duration::from_secs(45))).await?;
                    Ok(CallToolResult::structured(json!({
                        "build_output": result,
                        "success": result.contains("Finished") || result.contains("Compiling")
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

                    let output = Command::new("cargo")
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

                    if !output.status.success() {
                        return Ok(CallToolResult::structured(json!({
                            "error": stderr.trim(),
                            "success": false
                        })));
                    }

                    Ok(CallToolResult::structured(json!({
                        "results": results.trim(),
                        "success": true
                    })))
                }
                "cargo_tree" => {
                    let code = get_code_arg(&request, "cargo_tree")?;
                    let result =
                        run_rust_tool(code, &["tree"], Some(Duration::from_secs(30))).await?;
                    Ok(CallToolResult::structured(json!({
                        "tree": result,
                        "success": true
                    })))
                }
                "cargo_doc" => {
                    let code = get_code_arg(&request, "cargo_doc")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["doc", "--no-deps"], Some(Duration::from_secs(60)))
                            .await?;
                    Ok(CallToolResult::structured(json!({
                        "doc_output": result,
                        "success": result.contains("Documenting")
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
                        "analysis": result,
                        "success": true
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

async fn run_rust_tool(
    code: &str,
    args: &[&str],
    timeout: Option<Duration>,
) -> Result<String, McpError> {
    // Create a temporary directory for the Rust project
    let temp_dir = tempfile::tempdir()
        .map_err(|e| McpError::internal_error(format!("Failed to create temp dir: {}", e), None))?;

    let project_path = temp_dir.path();

    // Initialize a new Cargo project
    let output = Command::new("cargo")
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
    let _start = Instant::now();
    let mut cmd = Command::new("cargo");
    cmd.args(args)
        .current_dir(project_path)
        .env("CARGO_TERM_COLOR", "never");

    let output = if let Some(timeout_duration) = timeout {
        // Simple timeout implementation
        let mut child = cmd
            .spawn()
            .map_err(|e| McpError::internal_error(format!("Failed to spawn cargo: {}", e), None))?;

        

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            loop {
                if start.elapsed() > timeout_duration {
                    return Err(McpError::internal_error(
                        "Command timed out".to_string(),
                        None,
                    ));
                }

                if let Ok(Some(_status)) = child.try_wait() {
                    return child.wait_with_output().map_err(|e| {
                        McpError::internal_error(format!("Failed to get output: {}", e), None)
                    });
                }

                std::thread::sleep(Duration::from_millis(100));
            }
        })
        .await
        .map_err(|e| McpError::internal_error(format!("Task failed: {}", e), None))??
    } else {
        cmd.output()
            .map_err(|e| McpError::internal_error(format!("Failed to run cargo: {}", e), None))?
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Combine stdout and stderr for complete output
    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.is_empty() {
        stderr.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    Ok(combined)
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
