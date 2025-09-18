# Repository Guidelines

## Project Structure & Module Organization
- Root: `Cargo.toml` (workspace), `Cargo.lock`, `README.md`.
- Core library: `core/src/lib.rs` — shared database and server logic.
- Server binary: `server/src/main.rs` — MCP server entrypoint built on `rmcp`.
- Build output: `target/` (ignored).
- Scripts: `test-server.sh` for JSON-RPC smoke tests.
- Tests: none yet; add unit tests in modules and integration tests in `tests/`.

## Build, Test, and Development Commands
- `cargo build --bin rusty-tools-server` / `cargo build --release --bin rusty-tools-server`: compile debug/release binaries.
- `cargo check`: fast type-check without producing artifacts.
- `cargo fmt --all`: format the entire workspace with rustfmt.
- `cargo clippy --all-targets -- -D warnings`: lint; treat warnings as errors locally.
- `cargo test`: run unit/integration tests.
- `./target/release/rusty-tools-server`: run the MCP server binary.
- `./test-server.sh`: send initialize/tools requests to the server for smoke testing.

## Coding Style & Naming Conventions
- Formatting: rustfmt (4-space indent, default width). Run `cargo fmt --all` before commits.
- Linting: Clippy; fix or allow-with-justification; avoid `unsafe`.
- Names: `snake_case` for functions/modules, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for consts.
- Docs: prefer `///` module/item docs and concise comments explaining non-obvious logic.

## Testing Guidelines
- Framework: built-in Rust test harness via `cargo test`.
- Layout: unit tests under `#[cfg(test)]` in `core/src/` and `server/src/`; integration tests in `tests/` (e.g., `tests/mcp_smoke.rs`).
- Conventions: descriptive test names, arrange/act/assert structure, avoid external network.
- Coverage: no strict threshold; cover critical paths (tool routing, error handling, timeouts).
- Server smoke test: `./test-server.sh` (expects `target/release/rusty-tools-server`).

## Commit & Pull Request Guidelines
- Commits: use Conventional Commit style when possible.
  - Examples: `feat(server): add cargo_search tool`, `fix(timeout): prevent hang on long checks`, `docs: update README`.
- PRs: include summary, linked issues, screenshots or sample JSON-RPC I/O where helpful, and verification steps.
- Quality gate: run `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` before opening/merging.

## Security & Configuration Tips
- Tooling spawns `cargo` in temp dirs; avoid adding commands that execute arbitrary user code.
- Keep dependencies minimal; audit with `cargo install cargo-audit` then `cargo audit`.
- Ensure `rustfmt` and `clippy` components are installed via `rustup`.
