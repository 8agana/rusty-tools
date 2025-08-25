#!/bin/bash

# Simple debug script to test individual tool calls

echo "=== Simple Debug Test ==="
echo ""

# Clean up any existing database
rm -f rusty-tools.db

echo "Test 1: Single cargo_check with persist=true"
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_check","arguments":{"code":"fn main() { let x: i32 = \"hello\"; }","persist":true}}}' | ./target/release/rusty-tools | tail -1

echo ""
echo "Test 2: Check if database was created and populated"
if [ -f rusty-tools.db ]; then
    echo "✅ Database exists"
    if command -v sqlite3 &> /dev/null; then
        echo "Analyses count: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM analyses;')"
        echo "Errors count: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM errors;')"
        echo "Todos count: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos;')"
    fi
else
    echo "❌ Database does not exist"
fi

echo ""
echo "Test 3: Query cargo_history"
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_history","arguments":{}}}' | ./target/release/rusty-tools | tail -1

echo ""
echo "Test 4: Query cargo_todos"
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_todos","arguments":{}}}' | ./target/release/rusty-tools | tail -1

echo ""
echo "=== Debug complete ==="
