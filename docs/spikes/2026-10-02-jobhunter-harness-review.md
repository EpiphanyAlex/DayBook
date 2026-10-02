# Jobhunter harness 源码调研

> 调研日期：2026-10-02。方式：静态源码阅读，未运行 Jobhunter 测试或真实模型。
> 参考仓库：[`JR-Academy-AI/jobhunter-desktop`](https://github.com/JR-Academy-AI/jobhunter-desktop)。
> 源码版本：`6dc6f6069e24b284e15cdbfcd9448c602765b910`，调研时本机工作区无未提交改动。
> 规格落点：[01 Agent 运行时 §3.5](../prd/01-agent-runtime.md#runtime-harness)。本文保留参考依据，不作为 Daybook 已实现或验收通过的证据。

## 参考范围

参考外部 CLI 的适配、双向通信、模型能力查询与任务控制。Daybook 继续使用自身的 MCP、attempt、草稿和审计边界；外部项目的测试或实现不能替代 Daybook 的权限验证。后续源码变更需重新核查，不将该提交的行为视为上游长期契约。

## 源码观察与适用条件

表中的 R9/R10 均指 [01 Agent 运行时 §5](../prd/01-agent-runtime.md) 的待决项。

| 关注点 | 适用范围 | 源码观察与应用条件 |
|---|---|---|
| **共用执行流程** | 随 Codex 接入落实 | [Adapter](https://github.com/JR-Academy-AI/jobhunter-desktop/blob/6dc6f6069e24b284e15cdbfcd9448c602765b910/src-tauri/src/adapter.rs) 分离命令/输出差异；Daybook 共用超时、取消、进程组回收与结果收尾，各后端保留安装、认证及密封探测。输出/收尾的必要有界参数按 R9/R10 审定 |
| **统一会话事件** | Codex 切片落实 Rust 内部契约，不提前开放完整实时 UI | [Session](https://github.com/JR-Academy-AI/jobhunter-desktop/blob/6dc6f6069e24b284e15cdbfcd9448c602765b910/src-tauri/src/engine_session.rs) 将不同协议转换为统一消息。Daybook 区分原始诊断、结构化进度、协议完成、错误与权限请求；CLI 完成事件不等于 `complete_source` 成功，更不等于入账。携带任务身份并隔离迟到事件；最终字段/枚举由 R10 审定，不照搬 `Step` 类型 |
| **后端与模型能力分开管理** | 随统一选择落实 | [能力查询](https://github.com/JR-Academy-AI/jobhunter-desktop/blob/6dc6f6069e24b284e15cdbfcd9448c602765b910/src-tauri/src/engine_capability.rs) 的分层可借鉴；Daybook 分别表达安装、认证、额度、模型及解析就绪，查询失败保留未知。Claude 的预置别名不能冒充某账号已验证可用的模型清单 |
| **可靠双向通信** | 采用 `app-server` 时落实 | [Codex RPC](https://github.com/JR-Academy-AI/jobhunter-desktop/blob/6dc6f6069e24b284e15cdbfcd9448c602765b910/src-tauri/src/codex_rpc.rs) 的请求 ID 与通知暂存机制可借鉴。请求/响应须关联，等待响应期间不得丢失通知或反向请求；不支持的请求明确回复错误，额外权限明确拒绝。队列/消息长度/等待均有界，断流或协议错误不得冒充正常完成 |
| **事件通知＋状态快照** | 继续留在 M1 实时事件切片 | [Sink 与当前步骤快照](https://github.com/JR-Academy-AI/jobhunter-desktop/blob/6dc6f6069e24b284e15cdbfcd9448c602765b910/src-tauri/src/agent.rs) 可借鉴；按 [01 §3.4](../prd/01-agent-runtime.md) 绑定 `source_id` / `attempt_id` / session，界面重订阅从 Rust 取权威快照。UI 只观察，不因卸载或漏事件重新执行任务，完整快照/订阅契约仍由 R9 与 [03 审核 §3.8](../prd/03-review.md) 负责 |
| **不依赖 GUI 的验证** | Codex 切片落实 | [就绪验证与合成测试](https://github.com/JR-Academy-AI/jobhunter-desktop/blob/6dc6f6069e24b284e15cdbfcd9448c602765b910/src-tauri/src/engine_readiness.rs) 分离真实调用；两个 Daybook 后端共用假 CLI 生命周期契约测试，各自补协议与密封测试。假 CLI 通过不能证明真实 Codex 权限合格，真实验证仍单独记录 |

**不采纳的行为**：Jobhunter 的模型冷却/自动 fallback、应用内代理登录、运行中允许命令或文件操作的扩权界面、基于产物文件存在的续跑判定，不适用于当前 Daybook 规格。Daybook 不自动换付费账号或模型；额外能力不能靠弹窗授权突破五工具密封要求；恢复只认数据库中的 attempt、草稿与审计，显式重试创建新 attempt。Jobhunter 中丢弃 Codex stderr、无容量上限的通知队列和无独立时限的部分收尾等待也不照搬，须按本节与 [01 §3.4](../prd/01-agent-runtime.md) 补齐诊断和有界处理。
