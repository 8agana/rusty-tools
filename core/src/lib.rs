use anyhow::Result;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, InitializeRequestParam, InitializeResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParam, Resource, ServerCapabilities,
        ServerInfo, Tool,
    },
    service::{RequestContext, RoleServer},
};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::future::Future;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub enum PersistenceMode {
    Disabled,
    Path(PathBuf),
}

#[derive(Debug, Clone)]
pub struct ErrorInfo {
    code: Option<String>,
    message: String,
    file: Option<String>,
    line: Option<i32>,
    suggestion: Option<String>,
}

#[derive(Clone)]
pub struct RustyToolsServer {
    db: Option<Arc<Mutex<Database>>>,
}

impl RustyToolsServer {
    pub fn new(mode: PersistenceMode) -> Self {
        let db = match Database::new(mode.clone()) {
            Ok(Some(db)) => {
                match mode {
                    PersistenceMode::Path(path) => {
                        eprintln!("✅ Database initialized at: {}", path.display());
                    }
                    PersistenceMode::Disabled => {}
                }
                Some(Arc::new(Mutex::new(db)))
            }
            _ => {
                eprintln!("⚠️  Warning: Could not initialize database: Persistence disabled.");
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
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) = self.store_analysis_with_errors("cargo_fmt", &result, persist) {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "cargo_clippy" => {
                    eprintln!("🔧 Executing cargo_clippy");
                    let code = get_code_arg(&request, "cargo_clippy")?;
                    validate_rust_code(code)?;
                    let result = run_rust_tool(
                        code,
                        &["clippy", "--", "-D", "warnings"],
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
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) =
                        self.store_analysis_with_errors("cargo_clippy", &result, persist)
                    {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "cargo_check" => {
                    eprintln!("🔧 Executing cargo_check");
                    let code = get_code_arg(&request, "cargo_check")?;
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
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) = self.store_analysis_with_errors("cargo_check", &result, persist)
                    {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "rustc_explain" => {
                    eprintln!("🔧 Executing rustc_explain");
                    let error_code = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("error_code"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| McpError::invalid_params("error_code is required", None))?;

                    let output = StdCommand::new("rustc")
                        .args(["--explain", error_code])
                        .output()
                        .map_err(|e| {
                            McpError::internal_error(
                                format!("Failed to run rustc --explain: {}", e),
                                None,
                            )
                        })?;

                    let explanation = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                    let json_result = json!({
                        "error_code": error_code,
                        "explanation": explanation,
                        "stderr": stderr,
                        "success": output.status.success()
                    });

                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(!output.status.success()),
                    })
                }
                "cargo_fix" => {
                    eprintln!("🔧 Executing cargo_fix");
                    let code = get_code_arg(&request, "cargo_fix")?;
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
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) = self.store_analysis_with_errors("cargo_fix", &result, persist) {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "cargo_audit" => {
                    eprintln!("🔧 Executing cargo_audit");
                    let code = get_code_arg(&request, "cargo_audit")?;
                    validate_rust_code(code)?;
                    // cargo audit requires cargo-audit to be installed
                    let result =
                        run_rust_tool(code, &["audit"], Some(Duration::from_secs(60))).await?;
                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) = self.store_analysis_with_errors("cargo_audit", &result, persist)
                    {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "cargo_test" => {
                    eprintln!("🔧 Executing cargo_test");
                    let code = get_code_arg(&request, "cargo_test")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["test"], Some(Duration::from_secs(60))).await?;
                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) = self.store_analysis_with_errors("cargo_test", &result, persist)
                    {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "cargo_build" => {
                    eprintln!("🔧 Executing cargo_build");
                    let code = get_code_arg(&request, "cargo_build")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["build"], Some(Duration::from_secs(60))).await?;
                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) = self.store_analysis_with_errors("cargo_build", &result, persist)
                    {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "cargo_search" => {
                    eprintln!("🔧 Executing cargo_search");
                    let query = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("query"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| McpError::invalid_params("query is required", None))?;

                    let output = StdCommand::new("cargo")
                        .args(["search", query])
                        .output()
                        .map_err(|e| {
                            McpError::internal_error(
                                format!("Failed to run cargo search: {}", e),
                                None,
                            )
                        })?;

                    let results = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                    let json_result = json!({
                        "query": query,
                        "results": results,
                        "stderr": stderr,
                        "success": output.status.success()
                    });

                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(!output.status.success()),
                    })
                }
                "cargo_tree" => {
                    eprintln!("🔧 Executing cargo_tree");
                    let code = get_code_arg(&request, "cargo_tree")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["tree"], Some(Duration::from_secs(30))).await?;
                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) = self.store_analysis_with_errors("cargo_tree", &result, persist)
                    {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "cargo_doc" => {
                    eprintln!("🔧 Executing cargo_doc");
                    let code = get_code_arg(&request, "cargo_doc")?;
                    validate_rust_code(code)?;
                    let result =
                        run_rust_tool(code, &["doc"], Some(Duration::from_secs(60))).await?;
                    let json_result = json!({
                        "status": result.status,
                        "success": result.status == 0,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "duration_ms": result.duration_ms
                    });
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) = self.store_analysis_with_errors("cargo_doc", &result, persist) {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "rust_analyzer" => {
                    eprintln!("🔧 Executing rust_analyzer");
                    let code = get_code_arg(&request, "rust_analyzer")?;
                    validate_rust_code(code)?;
                    // rust-analyzer check
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
                    let persist = Self::get_persist_flag(&request);
                    if let Err(e) =
                        self.store_analysis_with_errors("rust_analyzer", &result, persist)
                    {
                        eprintln!("⚠️  Failed to store analysis: {}", e);
                    }
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(result.status != 0),
                    })
                }
                "cargo_history" => {
                    eprintln!("🔧 Executing cargo_history");
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
                        .unwrap_or(10) as usize;

                    let Some(ref db_arc) = self.db else {
                        return Err(McpError::internal_error("Database not available", None));
                    };

                    let db = db_arc.lock().map_err(|e| {
                        McpError::internal_error(format!("Database lock failed: {}", e), None)
                    })?;

                    let history = db.get_error_history(error_code, Some(limit)).map_err(|e| {
                        McpError::internal_error(format!("Failed to query history: {}", e), None)
                    })?;

                    let json_result = json!({
                        "error_code": error_code,
                        "limit": limit,
                        "results": history
                    });

                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(false),
                    })
                }
                "cargo_todos" => {
                    eprintln!("🔧 Executing cargo_todos");
                    let show_completed = request
                        .arguments
                        .as_ref()
                        .and_then(|args| args.get("show_completed"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let Some(ref db_arc) = self.db else {
                        return Err(McpError::internal_error("Database not available", None));
                    };

                    let db = db_arc.lock().map_err(|e| {
                        McpError::internal_error(format!("Database lock failed: {}", e), None)
                    })?;

                    let todos = db.get_todos(show_completed).map_err(|e| {
                        McpError::internal_error(format!("Failed to query todos: {}", e), None)
                    })?;

                    let json_result = json!({
                        "show_completed": show_completed,
                        "todos": todos
                    });

                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(false),
                    })
                }
                "db_stats" => {
                    eprintln!("🔧 Executing db_stats");
                    let Some(ref db_arc) = self.db else {
                        return Err(McpError::internal_error("Database not available", None));
                    };

                    let db = db_arc.lock().map_err(|e| {
                        McpError::internal_error(format!("Database lock failed: {}", e), None)
                    })?;

                    let stats = db.get_stats().map_err(|e| {
                        McpError::internal_error(format!("Failed to get stats: {}", e), None)
                    })?;

                    let json_result = json!(stats);

                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(json_result.to_string())],
                        structured_content: None,
                        meta: None,
                        is_error: Some(false),
                    })
                }
                _ => Err(McpError::internal_error(
                    format!("Unknown tool: {}", request.name),
                    None,
                )),
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

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(mode: PersistenceMode) -> Result<Option<Self>> {
        match mode {
            PersistenceMode::Disabled => Ok(None),
            PersistenceMode::Path(path) => {
                let conn = Connection::open(&path)?;

                // Create parent directory if it doesn't exist
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let db = Database { conn };
                db.init_schema()?;
                Ok(Some(db))
            }
        }
    }

    fn init_schema(&self) -> Result<()> {
        // Create analyses table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS analyses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                file_path TEXT,
                tool TEXT NOT NULL,
                full_output TEXT NOT NULL,
                success BOOLEAN NOT NULL
            )",
            [],
        )?;

        // Create errors table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS errors (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                analysis_id INTEGER NOT NULL,
                error_code TEXT,
                message TEXT NOT NULL,
                file TEXT,
                line INTEGER,
                suggestion TEXT,
                FOREIGN KEY (analysis_id) REFERENCES analyses (id)
            )",
            [],
        )?;

        // Create todos table - fix column type issues
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS todos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                source TEXT NOT NULL,
                description TEXT NOT NULL,
                file_path TEXT,
                line_number INTEGER,
                completed INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Create fixes table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS fixes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                error_id INTEGER,
                fix_applied TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                worked INTEGER,
                FOREIGN KEY (error_id) REFERENCES errors (id)
            )",
            [],
        )?;

        // Add timestamp column to existing errors table if it doesn't exist
        let _ = self.conn.execute(
            "ALTER TABLE errors ADD COLUMN timestamp DATETIME DEFAULT CURRENT_TIMESTAMP",
            [],
        );

        Ok(())
    }

    pub fn store_analysis(
        &self,
        tool: &str,
        full_output: &Value,
        success: bool,
        file_path: Option<&str>,
    ) -> Result<i64> {
        use rusqlite::params;
        let full_output_str = full_output.to_string();

        self.conn.execute(
            "INSERT INTO analyses (tool, full_output, success, file_path) VALUES (?1, ?2, ?3, ?4)",
            params![tool, full_output_str, success, file_path],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn store_error(
        &self,
        analysis_id: i64,
        error_code: Option<&str>,
        message: &str,
        file: Option<&str>,
        line: Option<i32>,
        suggestion: Option<&str>,
    ) -> Result<()> {
        use rusqlite::params;
        self.conn.execute(
            "INSERT INTO errors (analysis_id, error_code, message, file, line, suggestion) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                analysis_id,
                error_code,
                message,
                file,
                line,
                suggestion
            ]
        )?;
        Ok(())
    }

    pub fn store_todo(
        &self,
        source: &str,
        description: &str,
        file_path: Option<&str>,
        line_number: Option<i32>,
    ) -> Result<()> {
        use rusqlite::params;
        self.conn.execute(
            "INSERT INTO todos (source, description, file_path, line_number) VALUES (?1, ?2, ?3, ?4)",
            params![
                source,
                description,
                file_path,
                line_number
            ]
        )?;
        Ok(())
    }

    pub fn get_error_history(
        &self,
        error_code: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<ErrorRecord>> {
        use rusqlite::params;
        let limit = limit.unwrap_or(10) as i64;

        let mut errors = Vec::new();

        // Check if timestamp column exists in errors table
        let has_timestamp = self
            .conn
            .prepare("SELECT timestamp FROM errors LIMIT 1")
            .is_ok();

        if let Some(code) = error_code {
            let sql = if has_timestamp {
                "SELECT e.id, e.error_code, e.message, e.file, e.line, e.suggestion,
                        COALESCE(e.timestamp, a.timestamp) as timestamp, a.tool
                 FROM errors e
                 JOIN analyses a ON e.analysis_id = a.id
                 WHERE e.error_code = ?1
                 ORDER BY COALESCE(e.timestamp, a.timestamp) DESC
                 LIMIT ?2"
            } else {
                "SELECT e.id, e.error_code, e.message, e.file, e.line, e.suggestion,
                        a.timestamp, a.tool
                 FROM errors e
                 JOIN analyses a ON e.analysis_id = a.id
                 WHERE e.error_code = ?1
                 ORDER BY a.timestamp DESC
                 LIMIT ?2"
            };
            let mut stmt = self.conn.prepare(sql)?;
            let error_iter = stmt.query_map(params![code, limit], |row| {
                Ok(ErrorRecord {
                    id: row.get(0)?,
                    error_code: row.get::<_, Option<String>>(1)?,
                    message: row.get(2)?,
                    file: row.get::<_, Option<String>>(3)?,
                    line: row.get::<_, Option<i32>>(4)?,
                    suggestion: row.get::<_, Option<String>>(5)?,
                    timestamp: row.get(6)?,
                    tool: row.get(7)?,
                })
            })?;

            for error in error_iter {
                errors.push(error?);
            }
        } else {
            let sql = if has_timestamp {
                "SELECT e.id, e.error_code, e.message, e.file, e.line, e.suggestion,
                        COALESCE(e.timestamp, a.timestamp) as timestamp, a.tool
                 FROM errors e
                 JOIN analyses a ON e.analysis_id = a.id
                 ORDER BY COALESCE(e.timestamp, a.timestamp) DESC
                 LIMIT ?1"
            } else {
                "SELECT e.id, e.error_code, e.message, e.file, e.line, e.suggestion,
                        a.timestamp, a.tool
                 FROM errors e
                 JOIN analyses a ON e.analysis_id = a.id
                 ORDER BY a.timestamp DESC
                 LIMIT ?1"
            };
            let mut stmt = self.conn.prepare(sql)?;
            let error_iter = stmt.query_map(params![limit], |row| {
                Ok(ErrorRecord {
                    id: row.get(0)?,
                    error_code: row.get::<_, Option<String>>(1)?,
                    message: row.get(2)?,
                    file: row.get::<_, Option<String>>(3)?,
                    line: row.get::<_, Option<i32>>(4)?,
                    suggestion: row.get::<_, Option<String>>(5)?,
                    timestamp: row.get(6)?,
                    tool: row.get(7)?,
                })
            })?;

            for error in error_iter {
                errors.push(error?);
            }
        }

        Ok(errors)
    }

    pub fn get_todos(&self, show_completed: bool) -> Result<Vec<TodoRecord>> {
        let sql = if show_completed {
            "SELECT id, source, description, file_path,
                    CAST(line_number AS INTEGER) as line_number,
                    completed, created_at
             FROM todos
             ORDER BY created_at DESC"
        } else {
            "SELECT id, source, description, file_path,
                    CAST(line_number AS INTEGER) as line_number,
                    completed, created_at
             FROM todos
             WHERE completed = 0
             ORDER BY created_at DESC"
        };

        let mut stmt = self.conn.prepare(sql)?;
        let todo_iter = stmt.query_map([], |row| {
            // Handle line_number more carefully to avoid type issues
            let line_number: Option<i32> = match row.get::<_, Option<rusqlite::types::Value>>(4)? {
                Some(rusqlite::types::Value::Integer(i)) => Some(i as i32),
                Some(rusqlite::types::Value::Text(s)) => s.parse().ok(),
                Some(rusqlite::types::Value::Null) | None => None,
                _ => None,
            };

            Ok(TodoRecord {
                id: row.get(0)?,
                source: row.get(1)?,
                description: row.get(2)?,
                file_path: row.get::<_, Option<String>>(3)?,
                line_number,
                completed: row.get::<_, i32>(5)? != 0, // Convert INTEGER to bool
                created_at: row.get(6)?,
            })
        })?;

        let mut todos = Vec::new();
        for todo in todo_iter {
            todos.push(todo?);
        }
        Ok(todos)
    }

    #[allow(dead_code)]
    pub fn mark_todo_completed(&self, todo_id: i64) -> Result<()> {
        use rusqlite::params;
        self.conn.execute(
            "UPDATE todos SET completed = 1 WHERE id = ?1",
            params![todo_id],
        )?;
        Ok(())
    }

    /// Get statistics about stored data
    pub fn get_stats(&self) -> Result<DatabaseStats> {
        let analyses_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM analyses", [], |row| row.get(0))?;

        let errors_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM errors", [], |row| row.get(0))?;

        let todos_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE completed = 0",
            [],
            |row| row.get(0),
        )?;

        let completed_todos_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE completed = 1",
            [],
            |row| row.get(0),
        )?;

        Ok(DatabaseStats {
            total_analyses: analyses_count as usize,
            total_errors: errors_count as usize,
            active_todos: todos_count as usize,
            completed_todos: completed_todos_count as usize,
        })
    }

    /// Clean up old data beyond a certain limit
    #[allow(dead_code)]
    pub fn cleanup_old_data(&self, keep_analyses: usize) -> Result<()> {
        use rusqlite::params;

        // Delete old analyses and their associated errors
        self.conn.execute(
            "DELETE FROM errors WHERE analysis_id IN (
                SELECT id FROM analyses
                ORDER BY timestamp DESC
                LIMIT -1 OFFSET ?1
            )",
            params![keep_analyses],
        )?;

        self.conn.execute(
            "DELETE FROM analyses
             WHERE id NOT IN (
                SELECT id FROM analyses
                ORDER BY timestamp DESC
                LIMIT ?1
             )",
            params![keep_analyses],
        )?;

        Ok(())
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ErrorRecord {
    pub id: i64,
    pub error_code: Option<String>,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<i32>,
    pub suggestion: Option<String>,
    pub timestamp: String,
    pub tool: String,
}

#[derive(Debug, serde::Serialize)]
pub struct TodoRecord {
    pub id: i64,
    pub source: String,
    pub description: String,
    pub file_path: Option<String>,
    pub line_number: Option<i32>,
    pub completed: bool,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize)]
pub struct DatabaseStats {
    pub total_analyses: usize,
    pub total_errors: usize,
    pub active_todos: usize,
    pub completed_todos: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
    pub duration_ms: u128,
}

pub async fn run_rust_tool(
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
