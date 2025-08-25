#!/bin/bash

# Comprehensive persistence test - populate data then test retrieval

echo "=== Full Persistence Test ==="
echo ""

# Clean up existing database to start fresh
rm -f rusty-tools.db
echo "🧹 Cleaned up existing database"

echo ""
echo "Step 1: Populate database with cargo_check (E0308 error)"
echo "Request: cargo_check with mismatched types and persist=true"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_check","arguments":{"code":"fn main() { let x: i32 = \"hello\"; println!(\"{}\", x); }","persist":true}}}'
) | ./target/release/rusty-tools 2>/dev/null | tail -1 | jq -r '.result.success // .error // "No response"'

echo ""
echo "Step 2: Populate database with cargo_clippy (warnings)"
echo "Request: cargo_clippy with unused variable and persist=true"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_clippy","arguments":{"code":"fn main() { let unused_var = 42; println!(\"Hello\"); }","persist":true}}}'
) | ./target/release/rusty-tools 2>/dev/null | tail -1 | jq -r '.result.success // .error // "No response"'

echo ""
echo "Step 3: Check database was populated"
if [ -f rusty-tools.db ] && command -v sqlite3 &> /dev/null; then
    echo "✅ Database exists"
    echo "Analyses count: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM analyses;')"
    echo "Errors count: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM errors;')"
    echo "Todos count: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos;')"
    echo ""
    echo "Sample error codes:"
    sqlite3 rusty-tools.db "SELECT DISTINCT error_code FROM errors WHERE error_code IS NOT NULL;" | head -3
    echo ""
    echo "Sample todo sources:"
    sqlite3 rusty-tools.db "SELECT DISTINCT source FROM todos;" | head -3
else
    echo "❌ Database not found or sqlite3 not available"
fi

echo ""
echo "Step 4: Test cargo_history retrieval"
echo "Request: Get all error history"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_history","arguments":{"limit":5}}}'
) | ./target/release/rusty-tools 2>/dev/null | tail -1 | jq -r '.result.success // .result.error // .error // "No response"'

echo ""
echo "Step 5: Test cargo_history with specific error code"
echo "Request: Get E0308 errors specifically"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_history","arguments":{"error_code":"E0308","limit":3}}}'
) | ./target/release/rusty-tools 2>/dev/null | tail -1 | jq -r '.result.count // .result.error // .error // "No response"'

echo ""
echo "Step 6: Test cargo_todos retrieval"
echo "Request: Get active todos"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_todos","arguments":{"show_completed":false}}}'
) | ./target/release/rusty-tools 2>/dev/null | tail -1 | jq -r '.result.count // .result.error // .error // "No response"'

echo ""
echo "Step 7: Test db_stats"
echo "Request: Get database statistics"
(
    echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
    echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
    echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"db_stats","arguments":{}}}'
) | ./target/release/rusty-tools 2>/dev/null | tail -1 | jq -r '.result.stats.total_analyses // .result.error // .error // "No response"'

echo ""
echo "=== Raw Database Verification ==="
if [ -f rusty-tools.db ] && command -v sqlite3 &> /dev/null; then
    echo "Final database state:"
    echo "Total analyses: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM analyses;')"
    echo "Total errors: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM errors;')"
    echo "Active todos: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos WHERE completed = 0;')"
    echo "Completed todos: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos WHERE completed = 1;')"

    echo ""
    echo "Sample stored errors:"
    sqlite3 rusty-tools.db "SELECT error_code, substr(message, 1, 50) || '...' as message FROM errors LIMIT 3;" 2>/dev/null

    echo ""
    echo "Sample stored todos:"
    sqlite3 rusty-tools.db "SELECT source, substr(description, 1, 50) || '...' as description FROM todos LIMIT 3;" 2>/dev/null
fi

echo ""
echo "=== Test Results Summary ==="
echo "Expected outcomes:"
echo "✅ Database should be populated with analyses, errors, and todos"
echo "✅ cargo_history should return success without 'timestamp' column errors"
echo "✅ cargo_todos should return success without 'column type Text' errors"
echo "✅ All retrieval queries should work without schema mismatches"

if [ -f rusty-tools.db ] && command -v sqlite3 &> /dev/null; then
    ANALYSES_COUNT=$(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM analyses;')
    ERRORS_COUNT=$(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM errors;')
    TODOS_COUNT=$(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos;')

    echo ""
    if [ "$ANALYSES_COUNT" -gt 0 ] && [ "$ERRORS_COUNT" -gt 0 ]; then
        echo "🎉 SUCCESS: Database populated with $ANALYSES_COUNT analyses, $ERRORS_COUNT errors, $TODOS_COUNT todos"
    else
        echo "⚠️  WARNING: Database not properly populated (Analyses: $ANALYSES_COUNT, Errors: $ERRORS_COUNT, Todos: $TODOS_COUNT)"
    fi
fi

echo ""
echo "=== Full persistence test complete ==="
