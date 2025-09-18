#!/bin/bash

# Test script for database schema fixes

echo "=== Database Fixes Test ==="
echo ""

# Don't clean up existing database - we want to test with existing data
echo "Testing with existing database: rusty-tools.db"
if [ -f rusty-tools.db ]; then
    echo "✅ Found existing database"

    if command -v sqlite3 &> /dev/null; then
        echo "Database contents before fixes:"
        echo "Analyses: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM analyses;' 2>/dev/null || echo 'Error')"
        echo "Errors: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM errors;' 2>/dev/null || echo 'Error')"
        echo "Todos: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos;' 2>/dev/null || echo 'Error')"
        echo ""

        echo "Checking errors table schema:"
        sqlite3 rusty-tools.db "PRAGMA table_info(errors);" 2>/dev/null || echo "Error reading schema"
        echo ""

        echo "Sample todos data types:"
        sqlite3 rusty-tools.db "SELECT id, typeof(line_number), line_number FROM todos LIMIT 3;" 2>/dev/null || echo "Error reading todos"
        echo ""
    fi
else
    echo "❌ No existing database found - creating new one for testing"
fi

echo "Test 1: Testing cargo_history (should fix 'no such column: e.timestamp')"
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_history","arguments":{"limit":3}}}\n' | ./target/release/rusty-tools 2>&1 | grep -v "🚀\|✅\|🔧\|🛑" | tail -3

echo ""
echo "Test 2: Testing cargo_history with specific error code"
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_history","arguments":{"error_code":"E0308","limit":2}}}\n' | ./target/release/rusty-tools 2>&1 | grep -v "🚀\|✅\|🔧\|🛑" | tail -3

echo ""
echo "Test 3: Testing cargo_todos (should fix 'Invalid column type Text at index: 4')"
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_todos","arguments":{}}}\n' | ./target/release/rusty-tools 2>&1 | grep -v "🚀\|✅\|🔧\|🛑" | tail -3

echo ""
echo "Test 4: Testing cargo_todos with show_completed=true"
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cargo_todos","arguments":{"show_completed":true}}}\n' | ./target/release/rusty-tools 2>&1 | grep -v "🚀\|✅\|🔧\|🛑" | tail -3

echo ""
echo "Test 5: Verifying db_stats still works"
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}\n{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"db_stats","arguments":{}}}\n' | ./target/release/rusty-tools 2>&1 | grep -v "🚀\|✅\|🔧\|🛑" | tail -3

echo ""
if command -v sqlite3 &> /dev/null && [ -f rusty-tools.db ]; then
    echo "=== Final Database State ==="
    echo "Analyses: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM analyses;' 2>/dev/null || echo 'Error')"
    echo "Errors: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM errors;' 2>/dev/null || echo 'Error')"
    echo "Active todos: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos WHERE completed = 0;' 2>/dev/null || echo 'Error')"
    echo "Completed todos: $(sqlite3 rusty-tools.db 'SELECT COUNT(*) FROM todos WHERE completed = 1;' 2>/dev/null || echo 'Error')"

    echo ""
    echo "Updated errors table schema:"
    sqlite3 rusty-tools.db "PRAGMA table_info(errors);" 2>/dev/null || echo "Error reading schema"
fi

echo ""
echo "=== Database fixes test complete ==="
echo ""
echo "Expected results:"
echo "✅ cargo_history should return JSON with error history (no 'timestamp' column error)"
echo "✅ cargo_todos should return JSON with todos list (no 'column type Text' error)"
echo "✅ All tests should show success:true in their JSON responses"
