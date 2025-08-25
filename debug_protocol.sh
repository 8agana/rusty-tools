#!/bin/bash

# Test proper MCP protocol sequence

echo "=== MCP Protocol Debug Test ==="
echo ""

# Clean up any existing database
rm -f rusty-tools.db

echo "Test 1: Proper MCP sequence with cargo_check"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_check","arguments":{"code":"fn main() { let x: i32 = \"hello\"; }","persist":true}}}'
) | ./target/release/rusty-tools

echo ""
echo "Test 2: Check database contents"
if [ -f rusty-tools.db ] && command -v sqlite3 &> /dev/null; then
    echo "✅ Database exists"
    echo "Analyses: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM analyses;')"
    echo "Errors: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM errors;')"
    echo "Todos: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos;')"
    echo ""
    echo "Sample error entries:"
    sqlite3 rusty-tools.db "SELECT error_code, message FROM errors LIMIT 3;" 2>/dev/null || echo "No errors found"
else
    echo "❌ Database not found or sqlite3 not available"
fi

echo ""
echo "Test 3: Query history"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_history","arguments":{"limit":5}}}'
) | ./target/release/rusty-tools

echo ""
echo "Test 4: Query todos"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_todos","arguments":{}}}'
) | ./target/release/rusty-tools

echo ""
echo "=== Protocol test complete ==="
