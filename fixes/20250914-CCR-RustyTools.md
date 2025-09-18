# Critical Code Review — RustyTools MCP

Date: 2025-09-14
Repo: /Users/samuelatagana/Projects/LegacyMind/rusty-tools
Commit: 40083f975a13 (latest local)

## Executive Summary
RustyTools MCP builds and lints cleanly and provides a practical set of Rust toolchain helpers over MCP. The isolated temp‑project approach reduces state bleed across runs and the SQLite persistence adds useful history. The main areas to address are: runtime safety (test execution and networked commands), uniform timeouts, log hygiene, and minor API/version alignment with newer rmcp.

- Build: cargo check OK; cargo clippy --all-targets -- -D warnings OK; cargo test --no-run OK.
- MCP: stdio transport; tool schemas defined via rmcp::object!; list_resources returns empty.
- Persistence: SQLite at ~/.rusty-tools/rusty-tools.db with basic schema for analyses/errors/todos/fixes.

## High-Severity Findings
1) Test execution can run arbitrary user code
- Tool: cargo_test runs tests (executes user-provided code) inside a temp project.
- Current guard: validate_rust_code blocks certain strings (std::process::Command, std::fs::, std::net::, unsafe), but is easy to bypass (aliases/imports, spacing, macro indirection) and does not constrain build.rs.
- Risk: RCE, data exfiltration, network calls, or long-running processes when used on untrusted input.
- Recommendation (default-safe):
  - Gate execution behind env flag: RUSTY_TOOLS_ALLOW_TEST_EXEC=1. Default off → use cargo test --no-run to compile tests only, return diagnostics.
  - Add max wall clock (already present) and CPU time limits if feasible; kill child tree on timeout (already kills child).
  - Document trust model in README/AGENTS.md.
  - Acceptance: with flag unset, tests do not run; with flag set, existing behavior preserved with timeout.

2) Networked command without explicit gating
- Tool: cargo_search shells out to cargo search (hits crates.io).
- Risk: Hidden network dependence; in restricted environments this hangs or leaks queries.
- Recommendation: Add RUSTY_TOOLS_ALLOW_NETWORK=1 gate (default off) and a timeout (e.g., 8–10s) using tokio::time::timeout with a spawned process; return a clear error if disabled.

3) Missing timeouts on some tools
- cargo_fmt, cargo_search currently lack a timeout; other tools mostly pass a Duration.
- Recommendation: Standardize a default per-tool timeout (fmt=10s, clippy/check/tree=30s, build/doc=45–60s, test=60s) and allow override via env RUSTY_TOOLS_TIMEOUT_MS per tool or global.

4) Log hygiene: arguments include full code bodies
- Several paths log request.arguments with eprintln!, which may include the entire code string.
- Risk: Large, noisy logs; potential leakage if stderr is collected by callers.
- Recommendation: Truncate or omit code in logs (e.g., log length only, or first 120 chars with “…”) and avoid printing full JSON args. Keep stderr for diagnostics, but minimize content.

5) SQLite on async runtime without offloading
- Direct rusqlite calls occur on the async runtime under std::sync::Mutex; long DB ops can block.
- Recommendation: Wrap DB writes/reads in tokio::task::spawn_blocking where practical, or keep interactions short and batch writes. At minimum, add a comment justifying current approach and measure if needed.

## Medium-Severity Findings
6) rmcp version drift
- Repo uses rmcp = 0.6.0; other repos (surreal-mind) use 0.6.3.
- Recommendation: Plan upgrade to 0.6.3 to align types and reduce cross-repo friction. Validate RequestContext and macros.

7) Error parsing robustness
- parse_and_store_errors relies on string heuristics. For richer data, cargo check/clippy can emit JSON (message-format=json).
- Recommendation: Offer a mode that parses JSON diagnostics and stores structured fields (spans, labels) when available.

8) Database path permissions
- DB created under ~/.rusty-tools. Permissions are not explicitly tightened.
- Recommendation: After creating the directory/file, set 0700 on dir and 0600 on db file where OS supports it; mask path in logs.

9) Schema evolution notes
- Schema migrations are implicit (ALTER column guarded by a best-effort attempt). Consider a migrations table with versioning if schema grows.

10) Resource surface
- list_resources returns empty; that’s fine, but consider exposing quick links (README, AGENTS.md) or a small in-memory “usage tips” resource for clients that support it.

## Low-Severity / Nits
- Many eprintln! statements are useful but verbose; consider a RUSTY_TOOLS_VERBOSE flag.
- validate_rust_code pattern list can be documented as advisory only (not a security boundary).
- cargo-audit/rust-analyzer tools depend on system binaries; return helpful guidance (already do). Consider gating both behind ALLOW_NETWORK too.

## Strengths
- Isolated temp project per call prevents residue and enables clean, repeatable runs.
- Timeouts already implemented for most heavy tools, with kill on timeout.
- Persistence schema is simple and practical; helpful history queries (cargo_history, cargo_todos, db_stats) included.
- Stdio transport only; no HTTP surface to harden.

## Concrete Fix Plan (Phased)
Phase 1 — Safety & UX (quick wins)
- Add env gates: RUSTY_TOOLS_ALLOW_TEST_EXEC (default 0), RUSTY_TOOLS_ALLOW_NETWORK (default 0).
- Apply consistent per-tool timeouts; add timeout to fmt/search; expose RUSTY_TOOLS_TIMEOUT_MS.
- Sanitize logs: do not dump full arguments; truncate code previews.
- Return clear error when a gated operation is requested while disabled.

Phase 2 — Hardening & Alignment
- Upgrade rmcp to 0.6.3; adjust RequestContext usage if needed.
- Optional: offload DB read/writes via spawn_blocking; add brief benchmark in docs.
- Optional: set ~/.rusty-tools permissions to 0700 and DB to 0600 on creation; mask path in logs (print “…/.rusty-tools/rusty-tools.db”).

Phase 3 — Diagnostics & Docs
- Add JSON diagnostics mode for cargo check/clippy (—message-format=json) behind env flag; store structured spans.
- Document trust model, network/test gating, and timeouts in README/AGENTS.md.
- Add a simple integration test (feature-gated) that exercises cargo_check and cargo_fmt round-trip.

## Validation Checklist
- cargo clippy --all-targets -- -D warnings remains green.
- With gates disabled (default), cargo_test compiles tests without running; cargo_search returns gated error.
- fmt/search now respect timeouts and never hang.
- Logs no longer include full code bodies.

## Evidence Pointers
- Tool routing, schemas, and logs: src/main.rs (list_tools, call_tool).
- DB init/IO: src/database.rs.
- Build results: cargo check/clippy/test logs in this review session.

— End of report —

