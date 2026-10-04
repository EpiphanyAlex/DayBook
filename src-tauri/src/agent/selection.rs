use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSelection {
    pub backend_id: String,
    pub model_selection: ModelSelection,
}

impl Default for AgentSelection {
    fn default() -> Self {
        Self {
            backend_id: "claude-code".to_owned(),
            model_selection: ModelSelection::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum ModelSelection {
    Auto,
    Specific { model_id: String },
}

impl AgentSelection {
    pub fn validate(&self) -> AppResult<()> {
        if !matches!(self.backend_id.as_str(), "claude-code" | "codex") {
            return Err(AppError::invalid_argument("未知解析引擎"));
        }
        if let ModelSelection::Specific { model_id } = &self.model_selection {
            let valid = !model_id.is_empty()
                && model_id.len() <= 128
                && model_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
            if !valid {
                return Err(AppError::invalid_argument(
                    "模型 ID 只能包含字母、数字、-、_、.，最多 128 字节",
                ));
            }
            let dynamic = model_id == "default"
                || (self.backend_id == "claude-code"
                    && ["opus", "sonnet", "haiku", "fable"].contains(&model_id.as_str()));
            if dynamic {
                return Err(AppError::invalid_argument(
                    "固定模型请选择完整 ID；动态别名请使用自动模式",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_model_rejects_alias_and_control_characters() {
        for model_id in ["opus", "default", "id\n--tools", ""] {
            assert!(AgentSelection {
                backend_id: "claude-code".to_owned(),
                model_selection: ModelSelection::Specific {
                    model_id: model_id.to_owned()
                },
            }
            .validate()
            .is_err());
        }
        assert!(AgentSelection {
            backend_id: "claude-code".to_owned(),
            model_selection: ModelSelection::Specific {
                model_id: "claude-opus-5-5".to_owned()
            },
        }
        .validate()
        .is_ok());
        assert!(AgentSelection {
            backend_id: "codex".to_owned(),
            model_selection: ModelSelection::Specific {
                model_id: "default".to_owned()
            },
        }
        .validate()
        .is_err());
    }
}
