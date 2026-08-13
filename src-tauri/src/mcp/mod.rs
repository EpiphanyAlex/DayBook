use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    tool, tool_router, ErrorData as McpError,
};
use serde::Serialize;
use serde_json::Value;

use crate::{
    agent::{
        session::forward_to_main,
        types::{
            CompleteSourceArgs, DraftTransactionArgs, ListPendingSourcesArgs, ReadSourceArgs,
            ReportSourceTotalArgs,
        },
    },
    domain::draft::{ToolContent, ToolOutput},
};

#[derive(Debug, Clone, Default)]
pub struct DaybookMcp;

#[tool_router(server_handler)]
impl DaybookMcp {
    #[tool(description = "列出本次任务唯一被指派、等待解析的来源")]
    async fn list_pending_sources(
        &self,
        Parameters(args): Parameters<ListPendingSourcesArgs>,
    ) -> Result<CallToolResult, McpError> {
        forward("list_pending_sources", args).await
    }

    #[tool(description = "读取本次任务被指派来源的元数据与不可变证据原件")]
    async fn read_source(
        &self,
        Parameters(args): Parameters<ReadSourceArgs>,
    ) -> Result<CallToolResult, McpError> {
        forward("read_source", args).await
    }

    #[tool(description = "从来源证据中起草一笔交易；只能写草稿区，不能写事实表")]
    async fn draft_transaction(
        &self,
        Parameters(args): Parameters<DraftTransactionArgs>,
    ) -> Result<CallToolResult, McpError> {
        forward("draft_transaction", args).await
    }

    #[tool(
        description = "回报来源原件明确写出的合计；总共/一共/合计/总计/TOTAL 均不可漏报，且不能自行加总生成"
    )]
    async fn report_source_total(
        &self,
        Parameters(args): Parameters<ReportSourceTotalArgs>,
    ) -> Result<CallToolResult, McpError> {
        forward("report_source_total", args).await
    }

    #[tool(description = "声明本次来源已读完，并回报起草条数与未解析区域")]
    async fn complete_source(
        &self,
        Parameters(args): Parameters<CompleteSourceArgs>,
    ) -> Result<CallToolResult, McpError> {
        forward("complete_source", args).await
    }
}

async fn forward(tool: &str, args: impl Serialize) -> Result<CallToolResult, McpError> {
    let arguments = serde_json::to_value(args)
        .map_err(|error| McpError::invalid_params(format!("工具参数序列化失败：{error}"), None))?;
    match forward_to_main(tool, arguments).await {
        Ok(output) => Ok(output_to_mcp(output)),
        Err(error) => {
            let value = serde_json::to_value(&error).unwrap_or_else(|_| {
                serde_json::json!({
                    "code": "data.storage_failure",
                    "message": "工具错误序列化失败",
                    "detail": Value::Null,
                })
            });
            Ok(CallToolResult::error(vec![ContentBlock::text(
                value.to_string(),
            )]))
        }
    }
}

fn output_to_mcp(output: ToolOutput) -> CallToolResult {
    let content = output
        .content
        .into_iter()
        .map(|item| match item {
            ToolContent::Text { text } => ContentBlock::text(text),
            ToolContent::Image { data, mime_type } => ContentBlock::image(data, mime_type),
        })
        .collect();
    let mut result = CallToolResult::success(content);
    result.structured_content = output.structured_content;
    result
}
