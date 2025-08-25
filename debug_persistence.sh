#!/bin/bash

# Debug script to test persistence features directly

echo "=== Debug Persistence Test ==="
echo ""

# Clean up any existing database
rm -f rusty-tools.db

echo "Test 1: Test cargo_check with persist=true and intentional error"
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_check","arguments":{"code":"fn main() { let x: i32 = \"hello\"; }","persist":true}}}' | ./target/release/rusty-tools

echo ""
echo "=== Database should now exist ==="
ls -la rusty-tools.db 2>/dev/null || echo "❌ Database not created"

echo ""
echo "Test 2: Query cargo_history for E0308 errors"
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_history","arguments":{"error_code":"E0308","limit":5}}}' | ./target/release/rusty-tools

echo ""
echo "Test 3: Test cargo_todos"
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_todos","arguments":{}}}' | ./target/release/rusty-tools

echo ""
echo "Test 4: Test clippy with persist=true to generate todos"
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_clippy","arguments":{"code":"fn main() { let x = 1; println!(\"Hello\"); }","persist":true}}}' | ./target/release/rusty-tools

echo ""
echo "Test 5: Query todos again after clippy"
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_todos","arguments":{}}}' | ./target/release/rusty-tools

echo ""
echo "=== Debug complete ==="

# Show database contents if sqlite3 is available
if command -v sqlite3 &> /dev/null && [ -f rusty-tools.db ]; then
    echo ""
    echo "=== Database Contents ==="
    echo "Analyses table:"
    sqlite3 rusty-tools.db "SELECT id, tool, success, timestamp FROM analyses;"
    echo ""
    echo "Errors table:"
    sqlite3 rusty-tools.db "SELECT id, error_code, message FROM errors;"
    echo ""
    echo "Todos table:"
    sqlite3 rusty-tools.db "SELECT id, source, description, completed FROM todos;"
fi
