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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

mod database;

use database::Database;

#[derive(Debug, Clone)]
struct ErrorInfo {
    code: Option<String>,
    message: String,
    file: Option<String>,
    line: Option<i32>,
    suggestion: Option<String>,
}

#[derive(Clone)]
struct RustyToolsServer {
    db: Option<Arc<Mutex<Database>>>,
}

impl RustyToolsServer {
    fn new() -> Self {
        // Try to initialize database, but don't fail if it can't be created
        let db = match Database::new(None) {
            Ok(db) => {
                eprintln!("✅ Database initialized successfully");
                Some(Arc::new(Mutex::new(db)))
            }
            Err(e) => {
                eprintln!(
                    "⚠️  Warning: Could not initialize database: {}. Persistence will be disabled.",
                    e
                );
                None
            }
        };

        RustyToolsServer { db }
    }

    fn get_persist_flag(request: &CallToolRequestParam) -> bool {
        request
            .arguments
            .as_ref()
            .and_then(|args| args.get("persist"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Parse and store errors from stderr output
    fn parse_and_store_errors(db: &Database, analysis_id: i64, stderr: &str) {
        let mut error_count = 0;

        // Parse Rust compiler errors and warnings
        for line in stderr.lines() {
            if let Some(error_info) = Self::parse_error_line(line) {
                if let Err(e) = db.store_error(
                    analysis_id,
                    error_info.code.as_deref(),
                    &error_info.message,
                    error_info.file.as_deref(),
                    error_info.line,
                    error_info.suggestion.as_deref(),
                ) {
                    eprintln!("Failed to store error: {}", e);
                } else {
                    error_count += 1;
                }
            }
        }

        if error_count > 0 {
            eprintln!(
                "Stored {} errors from analysis {}",
                error_count, analysis_id
            );
        }
    }

    /// Enhanced error parsing that handles multiple error patterns
    fn parse_error_line(line: &str) -> Option<ErrorInfo> {
        let line = line.trim();

        // Pattern 1: error[E0308]: message
        if let Some(captures) = Self::extract_error_pattern(line, r"error\[([^\]]+)\]:\s*(.+)") {
            return Some(ErrorInfo {
                code: Some(captures.0),
                message: captures.1,
                file: None,
                line: None,
                suggestion: None,
            });
        }

        // Pattern 2: warning: message
        if let Some(captures) = Self::extract_error_pattern(line, r"warning:\s*(.+)") {
            return Some(ErrorInfo {
                code: Some("WARNING".to_string()),
                message: captures.1,
                file: None,
                line: None,
                suggestion: None,
            });
        }

        // Pattern 3: --> file:line:col (file path indicators)
        if line.contains(" --> ") && line.contains(':') {
            let parts: Vec<&str> = line.split(" --> ").collect();
            if parts.len() == 2 {
                let location = parts[1];
                if let Some(file_info) = Self::parse_file_location(location) {
                    return Some(ErrorInfo {
                        code: None,
                        message: format!("Error at {}", location),
                        file: Some(file_info.0),
                        line: file_info.1,
                        suggestion: None,
                    });
                }
            }
        }

        // Pattern 4: help: suggestion
        if line.starts_with("help:") {
            return Some(ErrorInfo {
                code: None,
                message: "Help".to_string(),
                file: None,
                line: None,
                suggestion: Some(
                    line.strip_prefix("help:")
                        .unwrap_or(line)
                        .trim()
                        .to_string(),
                ),
            });
        }

        None
    }

    /// Extract error code and message using regex-like pattern matching
    fn extract_error_pattern(line: &str, pattern: &str) -> Option<(String, String)> {
        // Simple pattern matching for error[CODE]: message
        if pattern.contains(r"error\[([^\]]+)\]")
            && let Some(error_start) = line.find("error[")
        {
            let after_error = &line[error_start..];
            if let Some(bracket_end) = after_error.find(']') {
                let code = after_error[6..bracket_end].to_string(); // Skip "error["
                if let Some(colon_pos) = after_error.find(": ") {
                    let message = after_error[colon_pos + 2..].trim().to_string();
                    return Some((code, message));
                }
            }
        }

        // Simple pattern matching for warning: message
        if pattern.contains(r"warning:\s*")
            && let Some(warning_pos) = line.find("warning:")
        {
            let message = line[warning_pos + 8..].trim().to_string();
            if !message.is_empty() {
                return Some(("WARNING".to_string(), message));
            }
        }

        None
    }

    /// Parse file location like "src/main.rs:10:5"
    fn parse_file_location(location: &str) -> Option<(String, Option<i32>)> {
        let parts: Vec<&str> = location.split(':').collect();
        if parts.len() >= 2 {
            let file = parts[0].to_string();
            let line = parts[1].parse::<i32>().ok();
            return Some((file, line));
        }
        None
    }

    /// Parse clippy warnings and store as todos
    fn parse_and_store_clippy_todos(db: &Database, stderr: &str) {
        let mut todo_count = 0;

        for line in stderr.lines() {
            let line = line.trim();

            // Clippy warnings often contain "warning:" and helpful suggestions
            if line.contains("warning:") && (line.contains("clippy::") || line.contains("#[warn("))
            {
                let warning_msg = if let Some(pos) = line.find("warning:") {
                    line[pos + 8..].trim()
                } else {
                    line
                };

                if !warning_msg.is_empty() {
                    if let Err(e) = db.store_todo("clippy", warning_msg, None, None) {
                        eprintln!("Failed to store clippy todo: {}", e);
                    } else {
                        todo_count += 1;
                    }
                }
            }

            // Store "help:" suggestions as todos too
            if line.starts_with("help:") {
                let help_msg = line.strip_prefix("help:").unwrap_or(line).trim();
                if !help_msg.is_empty() {
                    if let Err(e) = db.store_todo("clippy_help", help_msg, None, None) {
                        eprintln!("Failed to store clippy help: {}", e);
                    } else {
                        todo_count += 1;
                    }
                }
            }
        }

        if todo_count > 0 {
            eprintln!("Stored {} clippy todos", todo_count);
        }
    }

    /// Store analysis with improved error handling
    fn store_analysis_with_errors(
        &self,
        tool: &str,
        result: &ExecResult,
        persist: bool,
    ) -> Result<(), String> {
        if !persist {
            return Ok(());
        }

        let Some(ref db_arc) = self.db else {
            return Err("Database not initialized".to_string());
        };

        let db = db_arc
            .lock()
            .map_err(|e| format!("Database lock failed: {}", e))?;

        let json_result = json!({
            "status": result.status,
            "success": result.status == 0,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "duration_ms": result.duration_ms
        });

        match db.store_analysis(tool, &json_result, result.status == 0, None) {
            Ok(analysis_id) => {
                // Store errors from stderr
                Self::parse_and_store_errors(&db, analysis_id, &result.stderr);

                // Store clippy-specific todos if this was a clippy run
                if tool == "cargo_clippy" {
                    Self::parse_and_store_clippy_todos(&db, &result.stderr);
                }

                eprintln!(
                    "✅ Analysis {} stored successfully for {}",
                    analysis_id, tool
                );
                Ok(())
            }
            Err(e) => Err(format!("Failed to store analysis: {}", e)),
        }
    }
}

impl ServerHandler for RustyToolsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Rust development tools for formatting, linting, and analysis with persistence"
                    .into(),
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
        async move {
            eprintln!("🚀 MCP Server initialized");
            Ok(self.get_info())
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            eprintln!("📋 Listing tools");

            let tools = vec![
                Tool::new(
                    Cow::Borrowed("cargo_fmt"),
                    Cow::Borrowed("Format Rust code using rustfmt"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string", "description": "Rust code to format"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code to analyze"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code to check"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code to fix"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code with Cargo.toml to audit"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code with tests to run"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code to build-check"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code with dependencies to analyze"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code to generate documentation for"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
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
                            "code": {"type": "string", "description": "Rust code to analyze"},
                            "persist": {"type": "boolean", "description": "Store results in SQLite database", "default": false}
                        },
                        "required": ["code"]
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_history"),
                    Cow::Borrowed("Query past errors by error code from stored analyses"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "error_code": {"type": "string", "description": "Specific error code to search for (optional)"},
                            "limit": {"type": "number", "description": "Maximum number of results to return", "default": 10}
                        },
                        "required": []
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("cargo_todos"),
                    Cow::Borrowed("Show current todo list from warnings and clippy suggestions"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {
                            "show_completed": {"type": "boolean", "description": "Include completed todos", "default": false}
                        },
                        "required": []
                    })),
                ),
                Tool::new(
                    Cow::Borrowed("db_stats"),
                    Cow::Borrowed("Show database statistics and stored data counts"),
                    Arc::new(rmcp::object!({
                        "type": "object",
                        "properties": {},
                        "required": []
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
            eprintln!("🔧 Calling tool: {}", request.name);
            eprintln!("🔧 Tool arguments: {:?}", request.arguments);

            match request.name.as_ref() {
                "cargo_fmt" => {
                    eprintln!("🔧 Executing cargo_fmt");
                    let code = get_code_arg(&request, "cargo_fmt")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(code, &["fmt", "--", "--emit=stdout"], None).await?;
                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });
                    eprintln!("✅ cargo_fmt completed with status: {}", result.status);
                    Ok(CallToolResult::structured(json_result))
                }
                "cargo_clippy" => {
                    let code = get_code_arg(&request, "cargo_clippy")?;
                    let persist = Self::get_persist_flag(&request);
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["clippy", "--", "-W", "clippy::all"],
                        Some(Duration::from_secs(30)),
                    )
                    .await?;

                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });

                    // Store analysis if requested
                    if let Err(e) =
                        self.store_analysis_with_errors("cargo_clippy", &result, persist)
                    {
                        eprintln!("Persistence error: {}", e);
                    }

                    Ok(CallToolResult::structured(json_result))
                }
                "cargo_check" => {
                    let code = get_code_arg(&request, "cargo_check")?;
                    let persist = Self::get_persist_flag(&request);
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["check"], Some(Duration::from_secs(30))).await?;

                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });

                    // Store analysis if requested
                    if let Err(e) = self.store_analysis_with_errors("cargo_check", &result, persist)
                    {
                        eprintln!("Persistence error: {}", e);
                    }

                    Ok(CallToolResult::structured(json_result))
                }
                "rustc_explain" => {
                    eprintln!("🔧 Executing rustc_explain");
                    let error_code = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("error_code"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::invalid_params("error_code parameter required", None)
                        })?;

                    eprintln!("🔧 Explaining error code: {}", error_code);

                    let output = StdCommand::new("rustc")
                        .args(["--explain", error_code])
                        .output()
                        .map_err(|e| {
                            eprintln!("❌ Failed to run rustc: {}", e);
                            McpError::internal_error(format!("Failed to run rustc: {}", e), None)
                        })?;

                    let explanation = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    eprintln!("✅ rustc_explain completed");

                    if !output.status.success() && !stderr.is_empty() {
                        let result = json!({
                            "explanation": format!("Error: {}", stderr.trim()),
                            "success": false
                        });
                        eprintln!("🔧 Returning error result: {:?}", result);
                        return Ok(CallToolResult::structured(result));
                    }

                    let result = json!({
                        "status": 0,
                        "success": true,
                        "stdout": explanation.trim(),
                        "stderr": "",
                        "duration_ms": 0
                    });
                    eprintln!("🔧 Returning success result: {:?}", result);
                    Ok(CallToolResult::structured(result))
                }
                "cargo_fix" => {
                    let code = get_code_arg(&request, "cargo_fix")?;
                    let persist = Self::get_persist_flag(&request);
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["fix", "--allow-dirty"],
                        Some(Duration::from_secs(60)),
                    )
                    .await?;

                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });

                    if let Err(e) = self.store_analysis_with_errors("cargo_fix", &result, persist) {
                        eprintln!("Persistence error: {}", e);
                    }

                    Ok(CallToolResult::structured(json_result))
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
                    let persist = Self::get_persist_flag(&request);
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["test", "--", "--nocapture"],
                        Some(Duration::from_secs(60)),
                    )
                    .await?;

                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });

                    if let Err(e) = self.store_analysis_with_errors("cargo_test", &result, persist)
                    {
                        eprintln!("Persistence error: {}", e);
                    }

                    Ok(CallToolResult::structured(json_result))
                }
                "cargo_build" => {
                    let code = get_code_arg(&request, "cargo_build")?;
                    let persist = Self::get_persist_flag(&request);
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["build", "--message-format=short"],
                        Some(Duration::from_secs(45)),
                    )
                    .await?;

                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });

                    if let Err(e) = self.store_analysis_with_errors("cargo_build", &result, persist)
                    {
                        eprintln!("Persistence error: {}", e);
                    }

                    Ok(CallToolResult::structured(json_result))
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
                    let persist = Self::get_persist_flag(&request);
                    let result =
                        run_rust_tool(code, &["tree"], Some(Duration::from_secs(30))).await?;

                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });

                    if let Err(e) = self.store_analysis_with_errors("cargo_tree", &result, persist)
                    {
                        eprintln!("Persistence error: {}", e);
                    }

                    Ok(CallToolResult::structured(json_result))
                }
                "cargo_doc" => {
                    let code = get_code_arg(&request, "cargo_doc")?;
                    let persist = Self::get_persist_flag(&request);
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["doc", "--no-deps"], Some(Duration::from_secs(60)))
                            .await?;

                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });

                    if let Err(e) = self.store_analysis_with_errors("cargo_doc", &result, persist) {
                        eprintln!("Persistence error: {}", e);
                    }

                    Ok(CallToolResult::structured(json_result))
                }
                "rust_analyzer" => {
                    let code = get_code_arg(&request, "rust_analyzer")?;
                    let persist = Self::get_persist_flag(&request);
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

                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });

                    if let Err(e) =
                        self.store_analysis_with_errors("rust_analyzer", &result, persist)
                    {
                        eprintln!("Persistence error: {}", e);
                    }

                    Ok(CallToolResult::structured(json_result))
                }
                "cargo_history" => {
                    let error_code = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("error_code"))
                        .and_then(|v| v.as_str());

                    let limit = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("limit"))
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);

                    match &self.db {
                        Some(db) => match db.lock() {
                            Ok(db) => match db.get_error_history(error_code, limit) {
                                Ok(errors) => {
                                    let error_json: Vec<_> = errors
                                        .iter()
                                        .map(|e| {
                                            json!({
                                                "id": e.id,
                                                "error_code": e.error_code,
                                                "message": e.message,
                                                "file": e.file,
                                                "line": e.line,
                                                "suggestion": e.suggestion,
                                                "timestamp": e.timestamp,
                                                "tool": e.tool
                                            })
                                        })
                                        .collect();

                                    Ok(CallToolResult::structured(json!({
                                        "success": true,
                                        "count": error_json.len(),
                                        "errors": error_json
                                    })))
                                }
                                Err(e) => Ok(CallToolResult::structured(json!({
                                    "success": false,
                                    "error": format!("Database query failed: {}", e)
                                }))),
                            },
                            Err(e) => Ok(CallToolResult::structured(json!({
                                "success": false,
                                "error": format!("Could not access database: {}", e)
                            }))),
                        },
                        None => Ok(CallToolResult::structured(json!({
                            "success": false,
                            "error": "Database not initialized. No historical data available."
                        }))),
                    }
                }
                "cargo_todos" => {
                    let show_completed = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("show_completed"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    match &self.db {
                        Some(db) => match db.lock() {
                            Ok(db) => match db.get_todos(show_completed) {
                                Ok(todos) => {
                                    let todo_json: Vec<_> = todos
                                        .iter()
                                        .map(|t| {
                                            json!({
                                                "id": t.id,
                                                "source": t.source,
                                                "description": t.description,
                                                "file_path": t.file_path,
                                                "line_number": t.line_number,
                                                "completed": t.completed,
                                                "created_at": t.created_at
                                            })
                                        })
                                        .collect();

                                    Ok(CallToolResult::structured(json!({
                                        "success": true,
                                        "count": todo_json.len(),
                                        "todos": todo_json
                                    })))
                                }
                                Err(e) => Ok(CallToolResult::structured(json!({
                                    "success": false,
                                    "error": format!("Database query failed: {}", e)
                                }))),
                            },
                            Err(e) => Ok(CallToolResult::structured(json!({
                                "success": false,
                                "error": format!("Could not access database: {}", e)
                            }))),
                        },
                        None => Ok(CallToolResult::structured(json!({
                            "success": false,
                            "error": "Database not initialized. No todo data available."
                        }))),
                    }
                }
                "db_stats" => {
                    eprintln!("🔧 Executing db_stats");
                    match &self.db {
                        Some(db) => match db.lock() {
                            Ok(db) => match db.get_stats() {
                                Ok(stats) => {
                                    let result = json!({
                                        "success": true,
                                        "stats": {
                                            "total_analyses": stats.total_analyses,
                                            "total_errors": stats.total_errors,
                                            "active_todos": stats.active_todos,
                                            "completed_todos": stats.completed_todos
                                        }
                                    });
                                    eprintln!("✅ db_stats completed: {:?}", result);
                                    Ok(CallToolResult::structured(result))
                                }
                                Err(e) => {
                                    let result = json!({
                                        "success": false,
                                        "error": format!("Failed to get stats: {}", e)
                                    });
                                    eprintln!("❌ db_stats failed: {:?}", result);
                                    Ok(CallToolResult::structured(result))
                                }
                            },
                            Err(e) => {
                                let result = json!({
                                    "success": false,
                                    "error": format!("Could not access database: {}", e)
                                });
                                eprintln!("❌ db_stats lock failed: {:?}", result);
                                Ok(CallToolResult::structured(result))
                            }
                        },
                        None => {
                            let result = json!({
                                "success": false,
                                "error": "Database not initialized."
                            });
                            eprintln!("❌ db_stats no database: {:?}", result);
                            Ok(CallToolResult::structured(result))
                        }
                    }
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
    eprintln!("🚀 Rusty Tools MCP Server starting...");

    let handler = RustyToolsServer::new();
    let service = handler
        .serve(stdio())
        .await
        .map_err(|e| anyhow::anyhow!("failed to start server: {}", e))?;

    service.waiting().await?;

    eprintln!("🛑 Rusty Tools MCP Server shutting down");
    Ok(())
}
