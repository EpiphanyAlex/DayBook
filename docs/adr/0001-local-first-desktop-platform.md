# ADR-0001：本地优先桌面平台（Tauri v2 + React/TS + Rust）

- **日期**：2026-08-06
- **状态**：已接受
- **决策者**：@maintainer
- **相关**：[ADR-0003 Agent 运行时与可插拔后端](./0003-agent-runtime-and-pluggable-backend.md)、[ADR-0005 语音与系统集成](./0005-voice-and-system-integration.md)、[`docs/PRD.md` §3](../PRD.md)
- **2026-08-08 修订**：仓库转为公开，决策者署名改为非具名 handle。**决策内容未变。**
- **2026-08-09 修订**：决策句里的「React 18」改为「React」——主版本改在 [`00-foundation`](../prd/00-foundation.md) 初始化时锁定，见「决策」下的说明。**技术选型（Tauri v2 + React + Rust）未变。**
- **2026-08-13 修订**：「后果」同步 R6 定案后的进程拓扑：SQLite 只由 Tauri 主进程持有，agent CLI 与独立 Rust MCP helper 都由主进程管理；进程数变化不改变 Tauri command 的唯一应用边界。

## 背景

Daybook 要把「过去这段时间的钱和事」从截图等痕迹里重建出来。这件事对运行形态提出三个硬性要求：

1. **必须能调用用户本机已登录的 agent CLI**（Claude Code / Codex）。产品的成本模型建立在「用户自带 AI 额度」上（[`docs/PRD.md` §3](../PRD.md) 支点 1），而这些 CLI 的登录态、进程、文件访问权限都在本机。Web 应用做不到。
2. **账目与日程极私密**。「数据不出本机」既是产品承诺，也是「一个人能扛得动」的唯一形态——无服务器成本、无各国合规问题。
3. **审核界面是胜负手**（[`docs/PRD.md` §5.1](../PRD.md)）：密集表格 + 行内编辑 + 原文并排 + 键盘流 + 虚拟滚动，40 笔要在 30 秒内审完。

v1 目标平台只有 macOS。

## 决策

**桌面壳用 Tauri v2；界面用 React + TypeScript + Vite；系统能力、本地存储与进程管理由 Rust / Tauri command 提供。**

> **React 的主版本不在本 ADR 钉死**（2026-08-09 修订）：原文写「React 18」，但项目尚未初始化，没有 `package.json` 也没有任何依赖约束——**在没有兼容性理由的情况下预先锁一个旧主版本，是把一个本该在初始化那天做的选择提前做错**。主版本在 [`docs/prd/00-foundation.md`](../prd/00-foundation.md) 落地时锁定为当时的最新稳定主版本，并在 `package.json` 里固定；若届时有具体的兼容性理由要退回旧主版本，在该文登记。**本 ADR 要定的是「React 而不是 SwiftUI / Svelte」，不是版本号。**

具体地：

- **不创建** Electron 壳、内嵌 Node.js 本地服务、或任何 `localhost` HTTP API。前端与后端之间只走 Tauri command（IPC）。
- **不引入** 云服务、后端 API、账号体系、遥测、崩溃上报或第三方分析。唯一允许的出站流量是用户自己的 agent CLI 与其模型服务商之间的通信——由该 CLI 自行发起，**应用不代理、不转发、不记录**。
- 数据库文件放在**用户看得见、能备份**的位置，**不放 iCloud Drive**（会损坏 SQLite）。

修改这条平台决策需要先写新的 ADR。

## 理由

### 为什么不是原生 SwiftUI

先给 SwiftUI 公道话：PhotoKit / SpeechAnalyzer / 菜单栏 / 通知全是一等公民，二进制小，冷启动快，而 v1 本来就只做 Apple 生态。这些都是真实优势。

**但它在两件命门上吃亏**：

1. **审核界面**——密集表格 + 行内编辑 + 原文并排 + 键盘流 + 虚拟滚动是 web 的绝对主场。SwiftUI 的 `Table` 做这个要一路顶着 API 打。而审核界面是本产品全部价值的兑现处。
2. **回顾图表**——灵活度差一个量级。

再加两条：**开源可参与性**（贡献者不必装 Xcode）和 **Windows 后路**（改配置 vs 完全重写）。

**关于「Swift 的 AI coding 体验差」这一常见论断**：它真正卡在验证循环——`.xcodeproj` 是巨大 XML、`xcodebuild` 反馈慢、SwiftUI 运行时问题要肉眼看模拟器。**而本项目的 Swift 面积恰好完全避开这些**：单个 `.swift` 文件、`swiftc` 一行编译、命令行验证、无 UI、各约 150 行（照片库与语音 sidecar，见 [ADR-0005](./0005-voice-and-system-integration.md)）。**v1 更是零 Swift。** 所以这一条不作为否决 SwiftUI 的主要理由——上面两个命门才是。

### 为什么不是 Electron

- Tauri 二进制小一个数量级；
- Rust 侧做 SQLite / 进程管理 / 文件监听更扎实；
- 关键：`rmcp`（官方 Rust MCP SDK）让 **MCP server 可以用 Rust 写**（[ADR-0003](./0003-agent-runtime-and-pluggable-backend.md)），不必为它额外拉一个 Node 进程。（本条原文写「可以在同一进程内起」，2026-08-09 改——进程归属当时另有争议，**2026-08-12 由 R6 spike 定案为独立 helper 二进制**，见 [`docs/prd/01-agent-runtime.md` §3.1](../prd/01-agent-runtime.md)；**它仍然是 Rust 写的，不需要 Node**，本条论据不受影响。）

### 为什么不是「Tauri 壳 + 内嵌 Node 服务 / localhost API」

这是 Electron 思路的变体，会带来三个具体损失：① 多一个进程要管生命周期与端口冲突 ② `localhost` 端口是本机其他程序可见的攻击面，与「数据不出本机」的姿态相悖 ③ 类型安全从 Tauri command 的 Rust↔TS 桥退化成手写 HTTP 契约。

## 后果

**得到**：

- 审核界面与回顾图表用 web 技术栈，能力上限不受框架限制
- SQLite 连接只由 Tauri 主进程持有；主进程统一管理 agent CLI 与独立 Rust MCP helper 的生命周期，helper 经 Unix domain socket 请求受限操作且不直接触库
- 贡献者只需 Node + Rust 工具链，不需要 Xcode
- Windows/Linux 是「以后改配置」而非「以后重写」

**付出**：

- 二进制与冷启动不如原生 SwiftUI
- macOS 一等公民能力（PhotoKit、SpeechAnalyzer）要通过 sidecar 拿，多一层进程边界（[ADR-0005](./0005-voice-and-system-integration.md)）
- 团队要同时维护 Rust 与 TypeScript 两套门禁（对应 [`CLAUDE.md`](../../CLAUDE.md) 约束 16：两套测试门禁并列，任一失败即红）

**明确接受的代价**：无云同步意味着换机、多设备是用户自己用文件备份解决的问题，产品不提供任何辅助。
