#!/bin/bash

# Minimal test for a single tool execution

echo "=== Single Tool Test ==="
echo ""

# Clean up any existing database
rm -f rusty-tools.db

echo "Testing rustc_explain (no persistence, no async cargo calls):"
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rustc_explain","arguments":{"error_code":"E0308"}}}\n' | ./target/release/rusty-tools

echo ""
echo "Testing db_stats (database only, no cargo calls):"
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"db_stats","arguments":{}}}\n' | ./target/release/rusty-tools

echo ""
echo "Testing cargo_search (simple cargo call, no persistence):"
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_search","arguments":{"query":"serde"}}}\n' | ./target/release/rusty-tools

echo ""
echo "=== Test complete ==="
