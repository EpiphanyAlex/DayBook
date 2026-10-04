# Codex app-server 接入可行性（零额度）

> 日期：2026-10-02。被测本机 CLI：`codex-cli 0.154.0`（npm `@openai/codex`）；官方源码：`openai/codex` tag `rust-v0.154.0`，commit `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`。参考项目：Jobhunter `6dc6f6069e24b284e15cdbfcd9448c602765b910`，见[独立调研](./2026-10-02-jobhunter-harness-review.md)。Daybook 基线：`db60383418ee746d85a9a58c33940274cbffc796`。
>
> 只执行 `--help`、协议 schema 生成、源码检查和 app-server 只读 RPC；没有 `thread/start`、`turn/start`、`codex exec`、真实模型调用或读取个人截图/真值/报告。RPC 输出仅保留接口状态、字段名与计数，不保存账号标识、配置值或 token。
> 对照本机 `claude --version` 为 2.1.280；其 `--help` 有 `--model`，未列出模型清单查询命令。历史 Claude 密封验证版本见[旧 R6 spike](./2026-08-12-r6-agent-runtime.md)，本次没有重跑真实 Claude 探测。

## 步骤与观察

| 检查 | 零额度证据 | 结论与限制 |
|---|---|---|
| 入口与协议 | `codex --version`、`codex app-server --help`、`codex exec --help`；`codex app-server generate-json-schema --experimental --out /tmp/daybook-codex-schema-0154` | app-server 支持 stdio JSON-RPC；`exec --json` 只有事件 JSONL。app-server 的 `thread/start`、`turn/start`、`turn/interrupt` 和反向请求更适合受控任务。接口标为 experimental，须按版本检验。 |
| 图片 | Daybook `src-tauri/src/domain/draft.rs::read_source` 将 PNG/JPEG 编码为 MCP `image` block，`src-tauri/src/mcp/mod.rs` 原样转发；Codex `core/src/mcp_tool_call.rs::sanitize_mcp_tool_result_for_model` 对有 `Image` input modality 的模型保留该 block，不支持的模型用文字占位 | **静态链路可行**；没有真实 turn，尚不能证明所选模型看见 Daybook 图片，也不能证明五工具完整调用。必须用另行授权的合成截图验证。 |
| 五工具与完成 | Daybook MCP 注册 `list_pending_sources`、`read_source`、`draft_transaction`、`report_source_total`、`complete_source`；Codex 源码的 MCP tool routing 接收外部 MCP 调用结果 | 协议层有接入路径，但未运行真实 Codex 工具循环。Codex 的 `turn/completed` 不能替代 Daybook `complete_source` 与 attempt 收束。 |
| 账号与模型 | 只读 RPC `initialize`、`account/read`、`config/read`、`model/list`、`account/rateLimits/read` 均返回；本机账号存在，默认模型配置存在，模型列表当次 5 项，`ordinaryUsageAllowed=true` | 接口可查询，不代表某个模型可实际解析。`model/list` 的 `inputModalities`、`isDefault` 是候选信息；请求模型与实际模型仍需在 turn 元数据中验证，未知保持未知。认证归 CLI，本应用不得读取/保存凭证。额度查询失败或缺字段为未知。 |
| Claude 模型发现对照 | `claude` 2.1.280 的帮助包含 `--model`，未见机器可读模型清单命令；`claude models --help` 返回总帮助 | Claude 的具体模型候选/账号可用性不能用示例别名补成已验证清单。统一入口可以保留「自动」，具体模型发现或用户显式 ID 的契约仍需评审。 |
| 其他配置 | `config/read` 返回包含 `features`、`hooks`、`mcp_servers`、`plugins`、`skills` 的配置键；`hooks/list` 返回 1 项；`mcpServerStatus/list` 在 8 秒等待内未回应；源码 `core/src/tools/spec_plan.rs` 依模型、feature、环境和 MCP 绑定构建有效工具 | 配置与各局部列表不是**当前 turn 的完整有效能力 manifest**。协议请求列表没有能权威列出内置命令/文件工具、全部 MCP、插件、hook 与权限模式的统一接口。只读 sandbox 仍可能保留读文件/执行命令能力。**0.154.0 的权限闸门不能判通过**；密封方案和实际能力确认待解，未解前不得下发来源任务。 |
| 配置隔离 | `app-server --help` 没有 `exec` 的 `--ignore-user-config` / `--ignore-rules` / `--ephemeral`；`thread/start.config` 可覆盖部分配置，`exec` 可忽略用户配置，但没有五工具有效能力证明 | 不能把 Jobhunter 的 `exec --sandbox read-only` 或 `--ignore-user-config` 当成 Daybook 的密封实现。[官方仓库 0.154.0 issue](https://github.com/openai/codex/issues/45361)还报告 `thread/start.config` 后 `turn/start` 悬挂；本次未启动 turn，未在 macOS 复现或排除。 |
| RPC 生命周期 | schema 有请求 ID、通知、`ServerRequest`、`turn/interrupt`；本机只读 RPC 等待时收到一条 `remoteControl/status/changed` 通知 | 客户端必须按 ID 配对、排队处理通知并回复反向请求；错误与 EOF 分开；所有读取、队列、等待和回收有界。现有 Jobhunter `codex_rpc.rs` 有 ID/队列示例，但队列无容量上限、`stderr` 丢弃，不可照搬。取消、断流、后代持管道均未对真实 Codex 验证。 |

运行 `node scripts/spike-codex-rpc.mjs`（纯假 CLI、无真实 Codex 进程）得到 5/5 通过：乱序响应 ID、响应前通知与反向请求、超长行、事件队列溢出、EOF/请求超时区分、取消后等待请求拒绝与子进程回收。脚本仅为协议边界原型，尚未接入 Daybook Rust 运行时、MCP helper 或真实 Codex；其中 256 字节单行/4 条队列/短测试时限只是故意易触发的夹具参数，**不是生产容量决定**。

## 阻塞与下一步

**当前为协议可研究、产品接入未就绪。** 首要阻塞是[01 Agent 运行时 §3.7](../prd/01-agent-runtime.md)要求的完整、机器可读且权威的有效能力清单；0.154.0 的本机 schema 与固定源码未找到该接口。`mcpServerStatus/list` 即使返回也只覆盖 MCP，不能证明内置工具及 hook 消失。不能用模型自报、只读沙箱、配置快照或假 CLI 的模拟清单冒充真实权限验收。还要核实 app-server 传入密封配置后的 turn 能否完成。若后续版本仍不能证明有效能力面，则 Codex 后端保持不可就绪；可以继续实施不下发真实任务的统一选择与假 CLI 契约，但不得进入真实截图专项。

授权后的最小真实验证应在临时 Daybook 数据目录和完全合成 PNG 上进行：先固定 CLI 版本、模型 ID、启动配置与预期五工具；验证所选模型经 `read_source` 看见图片并调用 `draft_transaction`、可选的 `report_source_total`、`complete_source`；再逐项故意启用内置工具、用户 MCP、插件、hook/权限模式，要求真实进程的结构化证据使任务在下发前拒绝；最后做取消、超时与断流收尾。每次真实 turn 均消耗用户模型额度，须另行明确授权。9 图/56 笔真实截图专项和独立 M0 formal 均不属于这一步。
