use anyhow::Result;
use rmcp::{
    ErrorData as McpError,
    handler::server::ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, Implementation, InitializeRequestParam,
        InitializeResult, ListToolsResult, PaginatedRequestParam, ProtocolVersion,
        ServerCapabilities, ServerInfo, Tool, ToolsCapability,
    },
    service::{RequestContext, RoleServer, serve_server},
    transport::stdio,
};
use serde_json::json;
use std::borrow::Cow;
use std::future::Future;
use std::io::Write;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

#[derive(Clone)]
struct RustyToolsServer;

impl ServerHandler for RustyToolsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "rusty-tools".to_string(),
                version: "0.1.0".to_string(),
            },
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
                    let result = run_rust_tool(code, &["fmt", "--", "--emit=stdout"], None)?;
                    Ok(CallToolResult::structured(json!({
                        "formatted": result,
                        "success": true
                    })))
                }
                "cargo_clippy" => {
                    let code = get_code_arg(&request, "cargo_clippy")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["clippy", "--", "-W", "clippy::all"], Some(Duration::from_secs(30)))?;
                    Ok(CallToolResult::structured(json!({
                        "analysis": result,
                        "success": result.is_empty()
                    })))
                }
                "cargo_check" => {
                    let code = get_code_arg(&request, "cargo_check")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["check"], Some(Duration::from_secs(30)))?;
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
                        .ok_or_else(|| McpError {
                            code: rmcp::model::ErrorCode::INVALID_PARAMS,
                            message: "error_code parameter is required for rustc_explain tool".into(),
                            data: None,
                        })?;

                    validate_error_code(error_code)?;

                    let output = Command::new("rustc")
                        .args(["--explain", error_code])
                        .output()
                        .map_err(|e| McpError {
                            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
                            message: format!("Failed to execute rustc explain command: {}", e).into(),
                            data: None,
                        })?;

                    let explanation = String::from_utf8_lossy(&output.stdout);
                    Ok(CallToolResult::structured(json!({
                        "explanation": explanation.to_string(),
                        "error_code": error_code
                    })))
                }
                "cargo_fix" => {
                    let code = get_code_arg(&request, "cargo_fix")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["fix", "--allow-dirty"], Some(Duration::from_secs(60)))?;
                    Ok(CallToolResult::structured(json!({
                        "fixed": result,
                        "success": true
                    })))
                }
                "cargo_audit" => {
                    let code = get_code_arg(&request, "cargo_audit")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["audit"], Some(Duration::from_secs(30)))?;
                    Ok(CallToolResult::structured(json!({
                        "audit_results": result,
                        "vulnerabilities_found": result.contains("advisory")
                    })))
                }
                "cargo_test" => {
                    let code = get_code_arg(&request, "cargo_test")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["test"], Some(Duration::from_secs(60)))?;
                    Ok(CallToolResult::structured(json!({
                        "test_results": result,
                        "success": result.contains("test result: ok")
                    })))
                }
                "cargo_search" => {
                    let query = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("query"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| McpError {
                            code: rmcp::model::ErrorCode::INVALID_PARAMS,
                            message: "query parameter is required for cargo_search tool".into(),
                            data: None,
                        })?;

                    let output = Command::new("cargo")
                        .args(["search", query])
                        .output()
                        .map_err(|e| McpError {
                            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
                            message: format!("Failed to execute cargo search: {}", e).into(),
                            data: None,
                        })?;

                    let results = String::from_utf8_lossy(&output.stdout);
                    Ok(CallToolResult::structured(json!({
                        "search_results": results.to_string(),
                        "query": query
                    })))
                }
                "cargo_tree" => {
                    let code = get_code_arg(&request, "cargo_tree")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["tree"], Some(Duration::from_secs(30)))?;
                    Ok(CallToolResult::structured(json!({
                        "dependency_tree": result,
                        "success": true
                    })))
                }
                "cargo_doc" => {
                    let code = get_code_arg(&request, "cargo_doc")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["doc", "--no-deps"], Some(Duration::from_secs(45)))?;
                    Ok(CallToolResult::structured(json!({
                        "doc_output": result,
                        "success": result.contains("Documenting")
                    })))
                }
                "rust_analyzer" => {
                    let code = get_code_arg(&request, "rust_analyzer")?;
                    validate_rust_code(code)?;
                    let result = run_rust_analyzer(code)?;
                    Ok(CallToolResult::structured(json!({
                        "analysis": result,
                        "success": true
                    })))
                }
                _ => Err(McpError {
                    code: rmcp::model::ErrorCode::METHOD_NOT_FOUND,
                    message: format!("Unknown tool: {}. Available tools: cargo_fmt, cargo_clippy, cargo_check, rustc_explain, cargo_fix, cargo_audit, cargo_test, cargo_search, cargo_tree, cargo_doc, rust_analyzer", request.name).into(),
                    data: None,
                }),
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
        .ok_or_else(|| McpError {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            message: format!("code parameter is required for {} tool", tool_name).into(),
            data: None,
        })
}

fn validate_rust_code(code: &str) -> Result<(), McpError> {
    // Basic security validation
    if code.is_empty() {
        return Err(McpError {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            message: "Code cannot be empty".into(),
            data: None,
        });
    }

    if code.len() > 10_000 {
        return Err(McpError {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            message: "Code exceeds maximum length of 10,000 characters".into(),
            data: None,
        });
    }

    // Prevent potentially dangerous patterns
    let dangerous_patterns = [
        "std::process::Command",
        "std::fs::remove_dir_all",
        "std::fs::remove_file",
        "std::env::set_current_dir",
        "unsafe {",
        "#![allow(",
    ];

    for pattern in dangerous_patterns {
        if code.contains(pattern) {
            return Err(McpError {
                code: rmcp::model::ErrorCode::INVALID_PARAMS,
                message: format!("Code contains potentially dangerous pattern: {}", pattern).into(),
                data: None,
            });
        }
    }

    Ok(())
}

fn validate_error_code(error_code: &str) -> Result<(), McpError> {
    if error_code.is_empty() {
        return Err(McpError {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            message: "Error code cannot be empty".into(),
            data: None,
        });
    }

    if !error_code.starts_with('E') || error_code.len() < 4 || error_code.len() > 6 {
        return Err(McpError {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            message: format!(
                "Invalid error code format: {}. Expected format like E0308",
                error_code
            )
            .into(),
            data: None,
        });
    }

    // Validate that it's a numeric error code after 'E'
    if let Some(numbers) = error_code.get(1..)
        && numbers.parse::<u32>().is_err()
    {
        return Err(McpError {
            code: rmcp::model::ErrorCode::INVALID_PARAMS,
            message: format!(
                "Invalid error code format: {}. Expected numeric code after 'E'",
                error_code
            )
            .into(),
            data: None,
        });
    }

    Ok(())
}

fn run_rust_tool(
    code: &str,
    cargo_args: &[&str],
    timeout: Option<Duration>,
) -> Result<String, McpError> {
    // Create temporary file with the code
    let mut temp_file = NamedTempFile::new().map_err(|e| McpError {
        code: rmcp::model::ErrorCode::INTERNAL_ERROR,
        message: format!("Failed to create temporary file: {}", e).into(),
        data: None,
    })?;

    temp_file.write_all(code.as_bytes()).map_err(|e| McpError {
        code: rmcp::model::ErrorCode::INTERNAL_ERROR,
        message: format!("Failed to write to temporary file: {}", e).into(),
        data: None,
    })?;

    // Ensure the file is flushed and closed
    temp_file.flush().map_err(|e| McpError {
        code: rmcp::model::ErrorCode::INTERNAL_ERROR,
        message: format!("Failed to flush temporary file: {}", e).into(),
        data: None,
    })?;

    // Run cargo command with timeout protection
    let start_time = Instant::now();
    let mut child = Command::new("cargo")
        .args(cargo_args)
        .arg(temp_file.path())
        .spawn()
        .map_err(|e| McpError {
            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
            message: format!("Failed to spawn cargo process: {}", e).into(),
            data: None,
        })?;

    // Wait for process completion with optional timeout
    let output = if let Some(timeout_duration) = timeout {
        let mut elapsed = Duration::from_secs(0);
        while elapsed < timeout_duration {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    break;
                }
                Ok(None) => {
                    std::thread::sleep(Duration::from_millis(100));
                    elapsed = start_time.elapsed();
                    if elapsed >= timeout_duration {
                        let _ = child.kill();
                        return Err(McpError {
                            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
                            message: format!("Process timed out after {:?}", timeout_duration)
                                .into(),
                            data: None,
                        });
                    }
                }
                Err(e) => {
                    return Err(McpError {
                        code: rmcp::model::ErrorCode::INTERNAL_ERROR,
                        message: format!("Error waiting for process: {}", e).into(),
                        data: None,
                    });
                }
            }
        }
        child.wait_with_output().map_err(|e| McpError {
            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
            message: format!("Failed to get process output: {}", e).into(),
            data: None,
        })?
    } else {
        child.wait_with_output().map_err(|e| McpError {
            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
            message: format!("Failed to get process output: {}", e).into(),
            data: None,
        })?
    };

    // Temporary file is automatically cleaned up when temp_file goes out of scope

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    Ok(if output.status.success() {
        stdout.to_string()
    } else {
        format!("STDOUT:\n{}\n\nSTDERR:\n{}", stdout, stderr)
    })
}

fn run_rust_analyzer(code: &str) -> Result<String, McpError> {
    // Create temporary file with the code
    let mut temp_file = NamedTempFile::new().map_err(|e| McpError {
        code: rmcp::model::ErrorCode::INTERNAL_ERROR,
        message: format!("Failed to create temporary file: {}", e).into(),
        data: None,
    })?;

    temp_file.write_all(code.as_bytes()).map_err(|e| McpError {
        code: rmcp::model::ErrorCode::INTERNAL_ERROR,
        message: format!("Failed to write to temporary file: {}", e).into(),
        data: None,
    })?;

    temp_file.flush().map_err(|e| McpError {
        code: rmcp::model::ErrorCode::INTERNAL_ERROR,
        message: format!("Failed to flush temporary file: {}", e).into(),
        data: None,
    })?;

    // Try to use rust-analyzer CLI if available
    let output = Command::new("rust-analyzer")
        .args([
            "analysis-stats",
            "--quiet",
            temp_file.path().to_str().unwrap(),
        ])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let _stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                Ok(stdout.to_string())
            } else {
                // Fallback to basic cargo check if rust-analyzer fails
                run_rust_tool(
                    code,
                    &["check", "--message-format=json"],
                    Some(Duration::from_secs(30)),
                )
            }
        }
        Err(_) => {
            // Fallback to cargo check if rust-analyzer is not installed
            run_rust_tool(
                code,
                &["check", "--message-format=json"],
                Some(Duration::from_secs(30)),
            )
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Disable tracing output to prevent interference with MCP JSON-RPC protocol
    // MCP requires clean JSON output on stdout, but tracing outputs colored logs
    // that break JSON parsing on the client side

    let server = RustyToolsServer;
    let transport = stdio();

    // Log server start to stderr (won't interfere with MCP protocol)
    eprintln!("Rusty Tools MCP Server starting...");

    // Keep the server running indefinitely - MCP servers should not exit
    loop {
        match serve_server(server.clone(), transport.clone()).await {
            Ok(()) => {
                eprintln!("Server completed successfully, restarting...");
            }
            Err(e) => {
                eprintln!("Server error: {}, restarting...", e);
            }
        }
        // Brief pause before restarting to avoid tight loops
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
