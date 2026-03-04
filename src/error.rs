use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum FcpRustError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("LSP protocol error: {0}")]
    LspProtocol(String),

    #[error("LSP server error (code {code}): {message}")]
    LspServer { code: i64, message: String },

    #[error("parse error: {0}")]
    Parse(String),

    #[error("session error: {0}")]
    Session(String),

    #[error("resolver error: {0}")]
    Resolver(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, FcpRustError>;
