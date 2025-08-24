# rusty-tools

MCP (Model Context Protocol) server providing Rust development tools for AI assistants. Bridges the gap between AI code generation and Rust compiler validation.

## Why rusty-tools?

AI assistants often struggle with Rust's strict compiler and complex error messages. rusty-tools provides direct access to Rust toolchain commands through MCP, enabling:

- **Real-time validation** - Check code without leaving the chat
- **Error explanations** - Understand cryptic Rust errors
- **Code improvements** - Get clippy suggestions instantly
- **Format standardization** - Apply rustfmt consistently

## Installation

### For Claude Desktop

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "rusty-tools": {
      "command": "/path/to/rusty-tools/target/release/rusty-tools"
    }
  }
}
```

### Building from source

```bash
git clone https://github.com/8agana/rusty-tools.git
cd rusty-tools
cargo build --release
```

## Releases

Download prebuilt binaries from GitHub Releases and verify the checksum.

```bash
# Download latest binary and checksum
curl -L -o rusty-tools \
  https://github.com/8agana/rusty-tools/releases/latest/download/rusty-tools
curl -L -o rusty-tools-SHA256.txt \
  https://github.com/8agana/rusty-tools/releases/latest/download/rusty-tools-SHA256.txt

# Verify checksum (macOS)
shasum -a 256 -c rusty-tools-SHA256.txt

# Make executable and install
chmod +x rusty-tools
sudo mv rusty-tools /usr/local/bin/
```

Then point your MCP client (e.g., Claude Desktop) to `/usr/local/bin/rusty-tools`.

## Available Tools

### Core Tools (Production Ready)

- **cargo_build** - Build project (produces artifacts)
  ```rust
  // Compiles the temp project and reports compiler output
  // Use `cargo_check` for faster type-checking without artifacts
  ```

- **cargo_fmt** - Format Rust code using rustfmt
  ```rust
  // Input: unformatted code
  // Output: properly formatted code
  ```

- **cargo_clippy** - Analyze code for improvements
  ```rust
  // Returns suggestions for better patterns, performance, and style
  ```

- **cargo_check** - Type-check without building
  ```rust
  // Fast validation of code correctness
  ```

- **rustc_explain** - Explain compiler error codes
  ```rust
  // Input: "E0308"
  // Output: Detailed explanation of type mismatch error
  ```

- **cargo_fix** - Automatically fix compiler warnings
  ```rust
  // Returns diffs of suggested fixes
  ```

### Analysis Tools

- **rust_analyzer** - Deep code analysis and diagnostics (uses cargo check JSON)
- **cargo_tree** - Display dependency tree
- **cargo_doc** - Generate documentation

### Security & Dependencies

- **cargo_audit** - Scan for security vulnerabilities
- **cargo_search** - Search crates.io for packages

### Testing

- **cargo_test** - Run tests with visible output
  ```rust
  // Runs tests with --nocapture for full output visibility
  // Shows actual test results, not just pass/fail
  ```

## Use Cases

### For AI Assistants
- Validate generated Rust code instantly
- Understand compiler errors without context switching
- Apply formatting and best practices automatically

### For Developers
- Get compiler feedback directly in AI chat
- Learn from error explanations
- Maintain code quality with integrated clippy

### For Learning
- Understand Rust errors with clear explanations
- See formatting standards applied in real-time
- Learn best practices through clippy suggestions

## Architecture

Built with rmcp 0.6.0, rusty-tools creates isolated Rust environments for each tool invocation, ensuring:
- No persistent state between calls
- Safe execution without side effects
- Fast response times

## Limitations

Current limitations (by design for safety):
- No `cargo run` (binaries are never executed)
- Read-only operations on codebases

## Output Format

All tool responses follow a consistent shape:

```jsonc
{ "status": 0, "success": true, "stdout": "...", "stderr": "", "duration_ms": 123 }
```

Use `status`/`success` for reliable checks; parse `stdout` for compiler output.

## Contributing

Contributions welcome! Areas for improvement:
- Enhanced test output formatting
- Additional cargo subcommands
- Improved error message parsing

## License

MIT

## Author

Built by Sam Atagana ([@8agana](https://github.com/8agana)) - learned Rust in under 30 days to solve AI + Rust development friction.
