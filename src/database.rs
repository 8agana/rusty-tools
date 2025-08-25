use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(db_path: Option<&Path>) -> Result<Self> {
        let conn = if let Some(path) = db_path {
            Connection::open(path)?
        } else {
            // Default to rusty-tools.db in current directory
            Connection::open("rusty-tools.db")?
        };

        let db = Database { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        // Create analyses table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS analyses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                file_path TEXT,
                tool TEXT NOT NULL,
                full_output TEXT NOT NULL,
                success BOOLEAN NOT NULL
            )",
            [],
        )?;

        // Create errors table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS errors (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                analysis_id INTEGER NOT NULL,
                error_code TEXT,
                message TEXT NOT NULL,
                file TEXT,
                line INTEGER,
                suggestion TEXT,
                FOREIGN KEY (analysis_id) REFERENCES analyses (id)
            )",
            [],
        )?;

        // Create todos table - fix column type issues
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS todos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                source TEXT NOT NULL,
                description TEXT NOT NULL,
                file_path TEXT,
                line_number INTEGER,
                completed INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Create fixes table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS fixes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                error_id INTEGER,
                fix_applied TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                worked INTEGER,
                FOREIGN KEY (error_id) REFERENCES errors (id)
            )",
            [],
        )?;

        // Add timestamp column to existing errors table if it doesn't exist
        let _ = self.conn.execute(
            "ALTER TABLE errors ADD COLUMN timestamp DATETIME DEFAULT CURRENT_TIMESTAMP",
            [],
        );

        Ok(())
    }

    pub fn store_analysis(
        &self,
        tool: &str,
        full_output: &Value,
        success: bool,
        file_path: Option<&str>,
    ) -> Result<i64> {
        use rusqlite::params;
        let full_output_str = full_output.to_string();

        self.conn.execute(
            "INSERT INTO analyses (tool, full_output, success, file_path) VALUES (?1, ?2, ?3, ?4)",
            params![tool, full_output_str, success, file_path],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn store_error(
        &self,
        analysis_id: i64,
        error_code: Option<&str>,
        message: &str,
        file: Option<&str>,
        line: Option<i32>,
        suggestion: Option<&str>,
    ) -> Result<()> {
        use rusqlite::params;
        self.conn.execute(
            "INSERT INTO errors (analysis_id, error_code, message, file, line, suggestion) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                analysis_id,
                error_code,
                message,
                file,
                line,
                suggestion
            ]
        )?;
        Ok(())
    }

    pub fn store_todo(
        &self,
        source: &str,
        description: &str,
        file_path: Option<&str>,
        line_number: Option<i32>,
    ) -> Result<()> {
        use rusqlite::params;
        self.conn.execute(
            "INSERT INTO todos (source, description, file_path, line_number) VALUES (?1, ?2, ?3, ?4)",
            params![
                source,
                description,
                file_path,
                line_number
            ]
        )?;
        Ok(())
    }

    pub fn get_error_history(
        &self,
        error_code: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<ErrorRecord>> {
        use rusqlite::params;
        let limit = limit.unwrap_or(10) as i64;

        let mut errors = Vec::new();

        // Check if timestamp column exists in errors table
        let has_timestamp = self
            .conn
            .prepare("SELECT timestamp FROM errors LIMIT 1")
            .is_ok();

        if let Some(code) = error_code {
            let sql = if has_timestamp {
                "SELECT e.id, e.error_code, e.message, e.file, e.line, e.suggestion,
                        COALESCE(e.timestamp, a.timestamp) as timestamp, a.tool
                 FROM errors e
                 JOIN analyses a ON e.analysis_id = a.id
                 WHERE e.error_code = ?1
                 ORDER BY COALESCE(e.timestamp, a.timestamp) DESC
                 LIMIT ?2"
            } else {
                "SELECT e.id, e.error_code, e.message, e.file, e.line, e.suggestion,
                        a.timestamp, a.tool
                 FROM errors e
                 JOIN analyses a ON e.analysis_id = a.id
                 WHERE e.error_code = ?1
                 ORDER BY a.timestamp DESC
                 LIMIT ?2"
            };
            let mut stmt = self.conn.prepare(sql)?;
            let error_iter = stmt.query_map(params![code, limit], |row| {
                Ok(ErrorRecord {
                    id: row.get(0)?,
                    error_code: row.get::<_, Option<String>>(1)?,
                    message: row.get(2)?,
                    file: row.get::<_, Option<String>>(3)?,
                    line: row.get::<_, Option<i32>>(4)?,
                    suggestion: row.get::<_, Option<String>>(5)?,
                    timestamp: row.get(6)?,
                    tool: row.get(7)?,
                })
            })?;

            for error in error_iter {
                errors.push(error?);
            }
        } else {
            let sql = if has_timestamp {
                "SELECT e.id, e.error_code, e.message, e.file, e.line, e.suggestion,
                        COALESCE(e.timestamp, a.timestamp) as timestamp, a.tool
                 FROM errors e
                 JOIN analyses a ON e.analysis_id = a.id
                 ORDER BY COALESCE(e.timestamp, a.timestamp) DESC
                 LIMIT ?1"
            } else {
                "SELECT e.id, e.error_code, e.message, e.file, e.line, e.suggestion,
                        a.timestamp, a.tool
                 FROM errors e
                 JOIN analyses a ON e.analysis_id = a.id
                 ORDER BY a.timestamp DESC
                 LIMIT ?1"
            };
            let mut stmt = self.conn.prepare(sql)?;
            let error_iter = stmt.query_map(params![limit], |row| {
                Ok(ErrorRecord {
                    id: row.get(0)?,
                    error_code: row.get::<_, Option<String>>(1)?,
                    message: row.get(2)?,
                    file: row.get::<_, Option<String>>(3)?,
                    line: row.get::<_, Option<i32>>(4)?,
                    suggestion: row.get::<_, Option<String>>(5)?,
                    timestamp: row.get(6)?,
                    tool: row.get(7)?,
                })
            })?;

            for error in error_iter {
                errors.push(error?);
            }
        }

        Ok(errors)
    }

    pub fn get_todos(&self, show_completed: bool) -> Result<Vec<TodoRecord>> {
        let sql = if show_completed {
            "SELECT id, source, description, file_path,
                    CAST(line_number AS INTEGER) as line_number,
                    completed, created_at
             FROM todos
             ORDER BY created_at DESC"
        } else {
            "SELECT id, source, description, file_path,
                    CAST(line_number AS INTEGER) as line_number,
                    completed, created_at
             FROM todos
             WHERE completed = 0
             ORDER BY created_at DESC"
        };

        let mut stmt = self.conn.prepare(sql)?;
        let todo_iter = stmt.query_map([], |row| {
            // Handle line_number more carefully to avoid type issues
            let line_number: Option<i32> = match row.get::<_, Option<rusqlite::types::Value>>(4)? {
                Some(rusqlite::types::Value::Integer(i)) => Some(i as i32),
                Some(rusqlite::types::Value::Text(s)) => s.parse().ok(),
                Some(rusqlite::types::Value::Null) | None => None,
                _ => None,
            };

            Ok(TodoRecord {
                id: row.get(0)?,
                source: row.get(1)?,
                description: row.get(2)?,
                file_path: row.get::<_, Option<String>>(3)?,
                line_number,
                completed: row.get::<_, i32>(5)? != 0, // Convert INTEGER to bool
                created_at: row.get(6)?,
            })
        })?;

        let mut todos = Vec::new();
        for todo in todo_iter {
            todos.push(todo?);
        }
        Ok(todos)
    }

    #[allow(dead_code)]
    pub fn mark_todo_completed(&self, todo_id: i64) -> Result<()> {
        use rusqlite::params;
        self.conn.execute(
            "UPDATE todos SET completed = 1 WHERE id = ?1",
            params![todo_id],
        )?;
        Ok(())
    }

    /// Get statistics about stored data
    pub fn get_stats(&self) -> Result<DatabaseStats> {
        let analyses_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM analyses", [], |row| row.get(0))?;

        let errors_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM errors", [], |row| row.get(0))?;

        let todos_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE completed = 0",
            [],
            |row| row.get(0),
        )?;

        let completed_todos_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE completed = 1",
            [],
            |row| row.get(0),
        )?;

        Ok(DatabaseStats {
            total_analyses: analyses_count as usize,
            total_errors: errors_count as usize,
            active_todos: todos_count as usize,
            completed_todos: completed_todos_count as usize,
        })
    }

    /// Clean up old data beyond a certain limit
    #[allow(dead_code)]
    pub fn cleanup_old_data(&self, keep_analyses: usize) -> Result<()> {
        use rusqlite::params;

        // Delete old analyses and their associated errors
        self.conn.execute(
            "DELETE FROM errors WHERE analysis_id IN (
                SELECT id FROM analyses
                ORDER BY timestamp DESC
                LIMIT -1 OFFSET ?1
            )",
            params![keep_analyses],
        )?;

        self.conn.execute(
            "DELETE FROM analyses
             WHERE id NOT IN (
                SELECT id FROM analyses
                ORDER BY timestamp DESC
                LIMIT ?1
             )",
            params![keep_analyses],
        )?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct ErrorRecord {
    pub id: i64,
    pub error_code: Option<String>,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<i32>,
    pub suggestion: Option<String>,
    pub timestamp: String,
    pub tool: String,
}

#[derive(Debug)]
pub struct TodoRecord {
    pub id: i64,
    pub source: String,
    pub description: String,
    pub file_path: Option<String>,
    pub line_number: Option<i32>,
    pub completed: bool,
    pub created_at: String,
}

#[derive(Debug)]
pub struct DatabaseStats {
    pub total_analyses: usize,
    pub total_errors: usize,
    pub active_todos: usize,
    pub completed_todos: usize,
}
