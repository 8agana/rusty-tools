#!/bin/bash

# Test script to verify the Rusty Tools MCP server is working correctly

echo "Testing Rusty Tools MCP Server..."

# Test 1: Initialize request
echo "Test 1: Initialize request"
echo '{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}},"id":1}' | ./target/release/rusty-tools-server
echo ""

# Test 2: List tools request (separate server instance)
echo "Test 2: List tools request"
# Send initialize, then initialized notification, then tools/list over the same connection
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | ./target/release/rusty-tools-server

echo ""

# Test 3: Test a specific tool call (separate server instance)
echo "Test 3: Test cargo_fmt tool"
# Send initialize, then initialized, then tools/call
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_fmt","arguments":{"code":"fn main() { println!(\"Hello, world!\"); }"}}}' | ./target/release/rusty-tools-server

echo ""

echo "All tests completed."
