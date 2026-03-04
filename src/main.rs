use rmcp::{transport::stdio, ServiceExt};

mod mcp;
mod domain;
mod error;
mod fcpcore;
mod lsp;
mod resolver;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("fcp-rust starting");

    let server = mcp::server::RustServer::new();

    let service = server.serve(stdio()).await.inspect_err(|e| {
        eprintln!("MCP server error: {e}");
    })?;

    service.waiting().await?;

    Ok(())
}
