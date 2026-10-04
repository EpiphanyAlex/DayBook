# Claude Code 候选模型发现（零额度）

> 日期：2026-10-04。本机 `claude --version` 为 2.1.289；npm `@anthropic-ai/claude-agent-sdk` 0.3.289 的包元数据声明 `claudeCodeVersion` 为 2.1.289。只读取包的声明、启动空输入的 CLI 会话并调用 SDK `supportedModels()`；没有向模型发送提示词、执行工具调用或读取 Daybook 私有来源。

## 观察

`Query.supportedModels(): Promise<ModelInfo[]>` 是该固定版本 SDK 的类型声明。`ModelInfo` 包含 `value`、可选 `resolvedModel`、`displayName` 与能力描述。空输入会话返回 12 项候选，包括 `default`、`opus`、`sonnet`、`fable`、`haiku` 等别名及固定模型 ID；其中 `default` 在此时解析为 `claude-opus-5-5`。会话正常关闭，未产生模型 turn。

这只证明 CLI 能在该版本、该环境下返回候选，**不证明账户有权使用某项、额度充足、具体模型能看图，也不证明别名的解析结果长期不变**。Daybook 生产运行时为 Rust，不内嵌 Node SDK；要展示候选，须验证可由 CLI 的结构化初始化协议取得同等字段。协议不可用或返回字段无法验证时，模型清单为 `unknown`。`specific` 保存并下发固定 ID；别名只作显示信息，不存作固定选择。实际执行模型只能从该次结果的结构化元数据得出，缺失或多模型时保留未知。

既有 [Codex 可行性记录](./2026-10-02-codex-app-server-feasibility.md)中的 Claude 2.1.280 结论仅针对 `--help` 命令面；本记录增加 SDK 初始化发现证据，不回写旧记录。
