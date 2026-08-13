use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub detail: Option<Value>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: Value) -> Self {
        self.detail = Some(detail);
        self
    }

    pub fn storage(error: impl std::fmt::Display) -> Self {
        Self::new("data.storage_failure", "本地数据读写失败")
            .with_detail(serde_json::json!({ "reason": error.to_string() }))
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new("data.invalid_argument", message)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::storage(value)
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::storage(value)
    }
}
