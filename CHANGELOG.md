# Changelog

All notable changes to this project will be documented in this file.

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

