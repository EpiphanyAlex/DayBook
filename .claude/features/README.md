# .claude/features — 功能领域速查

> **这个目录回答一个问题：「这个功能现在是怎么实现的？」**
> 目的是让接手的 agent **免去逆向阅读代码**——不用为了改一行分类逻辑先读五个文件。

## 与其他文档的分工

| 目录 | 回答 | 时态 |
|---|---|---|
| [`docs/prd/`](../../docs/prd/) | **要什么、为什么**（规格） | 将来式 |
| [`docs/adr/`](../../docs/adr/) | **为什么这么定**（难以逆转的决定） | 完成时 |
| [`.claude/rules/`](../rules/) | **怎么写**（跨功能的实现细则） | 祈使句 |
| **`.claude/features/`（本目录）** | **现在是怎么实现的**（实况） | **现在式** |

**sub-PRD 说「应该」，feature 速查说「是」。** 两者不一致时，要么代码有 bug，要么 sub-PRD 该回流——**不是**在 feature 速查里写「理想情况下应该……」。

## 何时补

**功能首次落地时必须建对应文件**（仓库根 [`CLAUDE.md`](../../CLAUDE.md)「收尾三件事」第 3 条）；后续改动同步更新。**缺失即视为该功能未完成。**

## 文件命名

一个功能领域一个文件，`kebab-case.md`，名字对应用户能感知的能力，不是代码模块名：

```
ingest-screenshot.md      拖入截图到草稿生成
review-and-confirm.md     审核界面与确认入库
total-cross-check.md      总额交叉校验
agent-runtime.md          MCP server 与 agent 启动
money-and-currency.md     金额、汇率、三元组
memory-rules.md           记忆规则的产生与应用
items-lifecycle.md        事项生命周期
```

## 模板

```markdown
# <功能名>

> 规格：docs/prd/NN-xxx.md（换成真实 sub-PRD 的相对链接） · 最后更新：YYYY-MM-DD

## 一句话
这个功能做什么。

## 数据流
从用户动作到数据落库的完整路径，标出每一步在哪个文件。

用户拖入文件
  → src/components/DropZone.tsx
  → call('import_source')
  → src-tauri/src/commands/ingest.rs::import_source
  → src-tauri/src/domain/ingest.rs::import          ← 幂等判定在这里
  → src-tauri/src/store/sources.rs::insert

## 关键文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/domain/ingest.rs` | 幂等、状态机 |
| ... | ... |

## 数据结构
涉及的表与关键字段（只列与本功能相关的，不抄全表）。

## 业务规则
代码里实际执行的规则，尤其**不显然的那些**：
- 幂等以 SHA-256 内容哈希为准，不看文件名
- 证据先落盘后写库（反过来会产生悬空引用）

## 已知边界与坑
实现中踩过的、下一个人会再踩的。

## 相关
- ADR / rules / 其他 feature 的链接
```

## 写作要求

- **写实况，不写愿景**——没实现的功能不写进来，写「待建」也不行；那属于 sub-PRD
- **路径必须真实且带到文件级**——「在 domain 层」没有价值，`src-tauri/src/domain/ingest.rs::import` 才有
- **业务规则重点写「不显然的」**——从代码一眼能看出来的不用写，写那些「为什么这里要先 A 后 B」的
- **改了实现就改这里**，和改测试一样是同一个 PR 的事

## 当前状态

M0 端到端实现已落地，当前速查：

- [`money-and-currency.md`](./money-and-currency.md)
- [`agent-runtime.md`](./agent-runtime.md)
- [`ingest-screenshot.md`](./ingest-screenshot.md)
- [`total-cross-check.md`](./total-cross-check.md)
- [`review-and-confirm.md`](./review-and-confirm.md)
