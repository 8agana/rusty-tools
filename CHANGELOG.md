# Changelog

All notable changes to this project will be documented in this file.

## [0.3.1] - 2025-08-25

### Fixed
- **Database Type Mismatches**: Fixed "Invalid column type Text" errors by using proper parameter binding instead of string conversion for INTEGER columns (line_number and line fields)
- **Missing Error Parsing**: Added error parsing to rust_analyzer tool so errors are properly stored when --persist=true
- **SQLite Parameter Binding**: Replaced manual string conversion with rusqlite::params! macro for all database operations
- **Database Query Issues**: Fixed parameter type mismatches in get_error_history and other query methods

### Technical Details
- All database insert and update operations now use proper rusqlite parameter binding
- Option<i32> values (line numbers) are correctly handled as INTEGER types in SQLite
- Error parsing is now consistent across cargo_check, cargo_clippy, and rust_analyzer tools
- Improved database operation reliability and type safety

## [0.3.0] - 2025-08-24

### Added
- **SQLite Persistence**: Optional storage of tool analyses, errors, and todos
- **Database Schema**: Tables for analyses, errors, todos, and fixes with proper relationships
- **Optional --persist flag**: All existing tools now support optional result storage
- **cargo_history tool**: Query past errors by error code from stored analyses
- **cargo_todos tool**: Show current todo list from warnings and clippy suggestions
- **Error Parsing**: Automatic extraction of Rust compiler error codes and messages
- **Warning Processing**: Automatic todo creation from compiler warnings and clippy suggestions
- **Backward Compatibility**: All tools work exactly as before when --persist is not used

### Changed
- Tool schemas now include optional "persist" boolean parameter (defaults to false)
- RustyToolsServer now optionally initializes SQLite database (rusty-tools.db)
- Enhanced error handling with graceful degradation when database is unavailable

### Technical Details
- Uses rusqlite with bundled SQLite for zero external dependencies
- Database automatically created with proper schema on first run
- Thread-safe database access using Arc<Mutex<Database>>
- Structured error parsing for E-codes and compiler messages
- Clippy warning detection and todo extraction

## [0.2.0] - 2025-08-24

Changed
- Standardized tool outputs to a single shape: `{ status, success, stdout, stderr, duration_ms }`.
- All success checks now rely on process exit status (no string matching).
- `cargo_build` description clarified: it builds and produces artifacts. README updated to explain `cargo_build` vs `cargo_check`.
- Runtime now uses async `tokio::process::Command` for cargo invocations and concurrent pipe reads.

Added
- Duration tracking for each tool invocation (`duration_ms`).
- Output Format section in README documenting the response shape.

Fixed
- Proper timeout handling: on timeout, the child process is killed and awaited to avoid zombies.
- Prevent potential deadlocks by reading stdout/stderr concurrently.

## [0.1.0] - 2025-08-23

Initial release of rusty-tools MCP server with basic tools: cargo_fmt, cargo_clippy, cargo_check, rustc_explain, cargo_fix, cargo_audit, cargo_test, cargo_build, cargo_search, cargo_tree, cargo_doc, rust_analyzer.

