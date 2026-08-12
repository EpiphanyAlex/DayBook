# Spike 记录：R6 — MCP server 进程归属与密封启动配置

> 日期：**2026-08-12** · 对应 [`docs/prd/01-agent-runtime.md` §5](../prd/01-agent-runtime.md) R6 · 结论已回流该文 §3.1 与 §3.7
> 被测版本：**`claude` 2.1.228** · **`rmcp` 3.1.2** · rustc 1.97.0 · macOS Darwin 25.5.0
> 探针代码是一次性的，**不进仓库**。本文只留结论与复现方法。

## 这份文档为什么在这里

[`01-agent-runtime.md` §3.7](../prd/01-agent-runtime.md) 明确要求：spike 的**具体 flag 组合与已验证的 CLI 版本号**要写下来，但**不写进 sub-PRD**——CLI 的 flag 会变，规格跟着它腐烂；也不能不写，否则下一个人只能把这次考古重做一遍。

原定落点是 `.claude/features/agent-runtime.md`（随 M0 落地）。M0 尚未开工，而 [`.claude/features/README.md`](../../.claude/features/README.md) 要求那个目录**只写实况、路径带到文件级**——运行时还不存在，写进去就是违规。因此本次落在 `docs/spikes/`：**带日期的实测记录，会随被测版本过期，过期了就重跑而不是修补。**

**`.claude/features/agent-runtime.md` 建立时，把下面「密封配置」一节搬过去并链回本文。**

## 结论摘要

| 检查项 | 结论 |
|---|---|
| ① 三条候选与生命周期 | **选定候选 ①（独立 helper 二进制 + Unix domain socket）**；候选 ② 同样可行；候选 ③ 被 [`CLAUDE.md`](../../CLAUDE.md) 约束 1 挡住 |
| ② MCP 配置契约 | 已确认，见下 |
| ③ 厂商现行条款 | **不是绿灯**——当下可用，但厂商文档的措辞指向 API key，且该政策已被改过一次又撤回。详见 [`docs/PRD.md` §12](../PRD.md) |
| ④ 密封配置与能力探测 | **可做，但 hook 要靠「主动引发」而不是声明**；`input_schema` 后端不提供 |

## ① 进程归属：CLI 自己 fork，实测确认

工具返回 `PONG_FROM_MCP_PROCESS pid=13183 ppid=13172`，其中 13172 就是 `claude` 进程。**[§3.1](../prd/01-agent-runtime.md) 那条告示成立**：stdio 型 server 的启动方是 agent CLI 自己，不存在「连到一个已经在跑的进程」的形态。

| 候选 | 实测 |
|---|---|
| **① helper 二进制 + Unix domain socket** ← **选定** | ✅ 跑通。`PONG_FROM_APP_PROCESS`；站位主进程在 CLI 退出后仍存活，生命周期解耦 |
| ② 应用自身二进制 + `--mcp-stdio` 子命令 | ✅ 跑通，活动部件更少 |
| ③ Agent SDK / 库内嵌 | **未实测**：Rust 没有 Agent SDK，内嵌等于引入 Node/Python 运行时，[`CLAUDE.md`](../../CLAUDE.md) 约束 1 明文禁止；要走这条得先改 [ADR-0001](../adr/0001-local-first-desktop-platform.md) |

**选 ① 的理由是「谁写 SQLite」，不是进程拓扑。** 候选 ② 里 MCP server 与 Tauri 主进程是两个进程、都要碰 SQLite，需要 WAL 且草稿写完后主进程要被通知才能刷新 UI；候选 ① 把写入收敛到主进程一处，[`.claude/rules/rust-tauri.md`](../../.claude/rules/rust-tauri.md) §4 那条「`DraftStore` 根本没有写事实表的方法、越权在编译期不可表达」也只需在一个进程里成立。UDS 那点复杂度是一次性的，草稿区的一致性问题会持续到 M0 之后。

### 候选 ① 的两条实现约束

1. **macOS Unix socket 路径受 `SUN_LEN` 限制（约 104 字节）**，实测撞到过：`InvalidInput: path must be shorter than SUN_LEN`。`~/Library/Application Support/…` 下的路径已接近上限——**socket 不能直接放数据目录，要单独挑短路径**。
2. **没有优雅关闭。** server 随 CLI 退出而死、不留孤儿进程，但退出是被杀的：探针的 `mcp_server_stop` 事件**一次也没记录到**。**不要把任何 flush / 收尾逻辑挂在 MCP server 进程的退出路径上。**

## ② MCP 配置契约

- `--mcp-config` 接受**文件路径或内联 JSON 字符串**——内联可行，不必在磁盘上留配置文件
- 形状：`{"mcpServers":{"<name>":{"command":…,"args":[…],"env":{…}}}}`，`env` 确实传到子进程
- 工具命名空间：`mcp__<server>__<tool>`——`provider` 天然编码在工具名里
- **`--strict-mcp-config` 是必需的**：不带它，即使 `--setting-sources ""`，用户配置里的其他 MCP server 仍会混进来（实测：我们的 server + 3 个用户侧 server 同时在列）

## 密封配置（**会随 CLI 版本过期**）

**已验证于 `claude` 2.1.228。换版本必须重跑本节。**

```
--tools ""                                  内置工具 31 → 0
--strict-mcp-config --mcp-config <ours>     MCP server 只剩我们注入的那一个
--setting-sources ""                        plugins 12→0 · skills 64→0 · slash 106→0 · hook 不触发
--no-session-persistence                    会话不落盘
--disable-slash-commands
--allowedTools "mcp__daybook__…"            否则我们自己的工具会被权限拒绝
--output-format stream-json --verbose       探测所需
--include-hook-events                       探测所需，见下
```

实测结果：内置工具 **0** · MCP 工具**恰好只有我们注入的那一组** · plugins 0 · skills 0 · slash 0 · hook 不触发 · `apiKeySource: "none"`（**订阅登录仍可用**）。

### 三个反直觉的坑

1. **`--allowedTools` 不是可选项。** 少了它，密封配置下 `permissionMode=default` 会拒绝**我们自己的** MCP 工具：`"Claude requested permissions to use mcp__daybook__daybook_ping, but you haven't granted it yet."` 现象是「工具面看着完全正常、但一次也调不动」，很容易被误诊成 server 没起来。
2. **`--bare` 不是密封开关，是最小模式——它留下的恰好是 `Bash` / `Edit` / `Read`。** 正是 [§3.7](../prd/01-agent-runtime.md) 那条 `sqlite3` 绕过路径要的三件套。而且它的文档明说「OAuth and keychain are never read」，强制 `ANTHROPIC_API_KEY`，与 [`CLAUDE.md`](../../CLAUDE.md) 约束 11「不存储用户的 API key」正面冲突。**不要用。**
3. **`--safe-mode` 是反的。** 它把我们自己的 `--mcp-config` 一起杀掉（MCP server 变成空），却留着 30 个内置工具——**两件事都办反了。**

## ④ 能力清单探测

数据源是 `--output-format stream-json` 的 **`init` 握手消息**：结构化 JSON，**不经模型自述**，满足「机器可读」这条要件。

| [§3.7](../prd/01-agent-runtime.md) 要求覆盖 | 拿得到吗 | 字段 / 证据 |
|---|---|---|
| CLI 内置工具 | ✅ | `init.tools` 含 `Bash`/`Read`/`Edit`/`Write`；对抗测试故意放开 `Bash`，探测立刻看见 |
| 全部 MCP 工具及所属 server | ✅ | `init.mcp_servers`；基线里 6 个用户侧残留 server 全部现形 |
| 权限绕过模式 | ✅ | 对抗测试开 `bypassPermissions`，`init.permissionMode` 如实上报 |
| 插件 | ✅ | `init.plugins`（名字 + 路径 + 来源） |
| 自动记忆 / skills / agents / slash | ✅ | `init.memory_paths` · `skills` · `agents` · `slash_commands` |
| **hook** | ⚠️ **`init` 里没有** | 见下 |
| **工具的 `input_schema`** | ❌ **拿不到** | `init.tools` 只有名字 |

### hook：靠主动引发，不靠声明

对抗测试挂了一个 `PreToolUse` hook，它**确实执行了**（落盘证据 `HOOK_FIRED_AT_…`），而 `init` 对它只字未提。

可行的替代做法：`--include-hook-events` + **在探测会话里主动引发一次工具调用**，hook 就出现在流里：

```
hook_started  | PreToolUse:mcp__daybook__daybook_ping | PreToolUse
hook_response | PreToolUse:mcp__daybook__daybook_ping | PreToolUse | exit=0
```

**盲区，必须如实记着**：这只发现得了**能对我们的工具生效**的 hook。一个 `matcher: "Bash"` 的 hook 引不出来——不过密封配置里 `Bash` 根本不存在，它也无从生效。生命周期类 hook（`SessionStart` 等）无条件触发，同样可见。**本次未逐个验证** `Stop` / `SessionEnd` / `PreCompact` 是否也如实进流。

**关不掉、也看不见的一条**：企业策略（managed policy）设置始终生效——`--safe-mode` 的文档明说「Admin-managed (policy) settings still apply」。这是一条我们控制不了的 hook 来源，属于已知残余风险。

### 探测必须单独起一次进程（§3.7 问题 (a)：确认，规格的假定是对的）

实测：`--input-format stream-json` 保持 stdin 打开 8 秒、不发任何消息，**`init` 始终不来**——它只在收到提示之后才发出。**无法在「下发任务之前」于同一会话内完成验证。**

成本上有个便宜做法：探测进程用一个平凡提示启动，**读到 `init` 就杀掉**，全程约 1 秒、**零模型调用**。但**要连 hook 一起验就必须让那次工具调用真的发生**，那是一个真实 turn（实测约 $0.007，haiku）。

## 怎么重跑

1. 建一个最小 `rmcp` stdio server，注册一个回声工具，进程内记 `pid` / `ppid` / 启停时刻
2. 用上面的密封 flag 组合起 `claude -p`，`--mcp-config` 指向它
3. 读 `stream-json` 的 `init`，比对 `tools` / `mcp_servers` / `permissionMode` / `plugins`
4. 对抗用例三条：故意放开一个内置工具、故意开 `bypassPermissions`、故意挂一个 `PreToolUse` hook——**前两条必须在 `init` 里看见，第三条必须靠引发工具调用看见**
5. 换 CLI 版本后**重跑第 2–4 步**，并更新本文顶部的版本号
