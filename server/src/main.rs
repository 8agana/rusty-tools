use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use rusty_tools_core::{PersistenceMode, RustyToolsServer};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Log server start to stderr (won't interfere with MCP protocol)
    eprintln!("🚀 Rusty Tools MCP Server starting...");

    let mode = if let Ok(path) = std::env::var("RUSTY_TOOLS_DB_PATH") {
        PersistenceMode::Path(PathBuf::from(path))
    } else {
        // Default to ~/.rusty-tools/rusty-tools.db or XDG_DATA_HOME
        let default_path = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".rusty-tools").join("rusty-tools.db"))
            .or_else(|_| {
                std::env::var("XDG_DATA_HOME")
                    .map(|x| PathBuf::from(x).join("rusty-tools").join("rusty-tools.db"))
            })
            .unwrap_or_else(|_| {
                eprintln!("⚠️  No HOME or XDG_DATA_HOME set, using current directory for DB");
                PathBuf::from("rusty-tools.db")
            });
        PersistenceMode::Path(default_path)
    };

    let handler = RustyToolsServer::new(mode);
    let service = handler
        .serve(stdio())
        .await
        .map_err(|e| anyhow::anyhow!("failed to start server: {}", e))?;

    service.waiting().await?;

    eprintln!("🛑 Rusty Tools MCP Server shutting down");
    Ok(())
}
