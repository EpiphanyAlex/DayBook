//! 评测评分器（[`docs/prd/07-eval.md`](../../../docs/prd/07-eval.md)）。
//!
//! **本模块只做零额度、确定性的那一半**：解析 manifest 与真值、按 `source_ordinal`
//! 做 full outer join、算计数、重放夹具的工具调用序列。**真跑 agent 的 eval 轮次
//! （烧额度、不进 CI）不在这里**——那一半在 07 §3.1，属后续交付。
//!
//! ## 两条由现有门禁倒推出来的形状
//!
//! 1. **不许出现浮点。** `scripts/verify-m0.mjs` 的「金额代码无浮点类型」那条禁用模式
//!    覆盖整个 `src-tauri/src`，本目录也在内。于是每个比率都是 [`metrics::Ratio`]
//!    这个整数对，阈值判定用交叉相乘。**这不是将就**——[`docs/PRD.md`] §9.4 本来就
//!    要求「每个比率一律连原始计数一起报（`0.967 (58/60)`）」，浮点反而会把原始
//!    计数丢掉。格式化留给 `scripts/eval.mjs`。
//! 2. **测试模块必须叫 `eval`。** 验收选择器（07 §6）写成 `cargo test
//!    eval::alignment_uses_reported_ordinal`，而 `verify-m0.mjs` 是**子串**匹配
//!    `cargo test -- --list` 的输出。`src/eval/join.rs` 里的 `#[cfg(test)] mod eval`
//!    产出 `eval::join::eval::alignment_uses_reported_ordinal`，含那个子串。
//!
//! ## 真值与预测分别是什么（07 §3.2）
//!
//! - **真值** = `expected.json`，人工标注，唯一 ground truth
//! - **预测** = `draft_transactions.drafted_json`，**不是草稿行的当前值**——行内编辑
//!   会就地改掉当前值，读当前值算出来的错误率恒为零
//! - **人的修改** = `audit_log` 里 `actor = "human"` 的行，不参与准确率计算

pub mod expected;
pub mod export;
pub mod join;
pub mod manifest;
pub mod metrics;
pub mod replay;
pub mod report;

use std::fmt;

use crate::error::AppError;

/// eval 自己的错误类型。
///
/// **刻意不复用 [`AppError`] 的码表**：[`docs/prd/00-foundation.md`] §3.7 那张表是
/// **IPC 契约**的码集（前端按 `code` 分支），而 eval 是开发工具，一个字节都不过 IPC。
/// 往那张表里加 `eval.*` 会让「前端能收到哪些码」这件事变得说不清。
///
/// 从 `DraftStore` / `domain::confirm` 冒上来的 `AppError` 原样包住——那些是被测系统
/// 的真实反应，改写它等于伪造证据。
#[derive(Debug)]
pub enum EvalError {
    /// 夹具与当前代码不同代（07 §5 R4）。**必须报得明白，不是重放到一半报个别的错。**
    StaleFixture {
        field: &'static str,
        expected: String,
        found: String,
    },
    /// manifest 自身不合法：缺字段、路径不存在、分池标记缺失。
    Manifest(String),
    /// 命令行用法错误。**和 manifest 错误分开**——「缺子命令」被冠上「manifest 不合法」
    /// 会把使用者引到一个根本没问题的文件上。
    Usage(String),
    /// 夹具目录内容不合法。
    Fixture(String),
    /// 被测系统返回的错误，原样透传。
    App(AppError),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for EvalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleFixture {
                field,
                expected,
                found,
            } => write!(
                formatter,
                "夹具已过期：{field} 期望 {expected}，夹具里是 {found}。重新导出夹具，不要改这个字段"
            ),
            Self::Manifest(message) => write!(formatter, "manifest 不合法：{message}"),
            Self::Usage(message) => write!(formatter, "{message}"),
            Self::Fixture(message) => write!(formatter, "夹具不合法：{message}"),
            Self::App(error) => write!(formatter, "{}（{}）", error.message, error.code),
            Self::Io(error) => write!(formatter, "读写失败：{error}"),
            Self::Json(error) => write!(formatter, "JSON 解析失败：{error}"),
        }
    }
}

impl std::error::Error for EvalError {}

impl From<AppError> for EvalError {
    fn from(error: AppError) -> Self {
        Self::App(error)
    }
}

impl From<std::io::Error> for EvalError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for EvalError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub type EvalResult<T> = Result<T, EvalError>;

// 名字不叫 `eval`：`mod eval` 套在 `eval` 里会撞 clippy 的 `module_inception`，
// 而 `-D warnings` 下那是一条错误。下面两条是结构断言，不在 07 §6 的选择器清单里，
// 所以换个模块名不影响任何一条验收。
#[cfg(test)]
mod eval_guards {
    /// 07 §6：「重放路径上没有 spawn 子进程」。
    ///
    /// 计数式断言（见 `replay.rs` 的 `replay_does_not_invoke_agent`）证明**这一次**没
    /// spawn；这一条证明**根本没有那条路**。需要的字面量放在本文件里而不是 `replay.rs`
    /// 里，否则断言自己会命中自己。
    #[test]
    fn replay_path_cannot_reach_the_agent() {
        let source = include_str!("replay.rs");
        for needle in [
            concat!("run_", "task"),
            concat!("Agent", "Runtime"),
            concat!("Agent", "Backend"),
            concat!("Command", "::new"),
            concat!("std::", "process"),
            concat!("spawn", "_sealed"),
        ] {
            assert!(
                !source.contains(needle),
                "重放路径不得引用 `{needle}`——夹具重放测的是「agent 读错时闸门有没有拦住」，不是模型"
            );
        }
    }

    /// 07 §6：「实现里出现『按 `evidence_text` 在原件上定位』的路径即红」。
    ///
    /// 系统里没有 OCR，对一张 PNG 无从知道某段文字在图上哪里（07 §3.2）。对齐只能靠
    /// agent 自报的 `source_ordinal`。
    #[test]
    fn alignment_never_locates_by_evidence_text() {
        let source = include_str!("join.rs");
        assert!(
            !source.contains(concat!("evidence", "_text")),
            "对齐模块不得触及 evidence_text——那条路做不到（没有 OCR），07 §3.2"
        );
    }
}
