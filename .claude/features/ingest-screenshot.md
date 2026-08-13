# 导入截图与口述

> 规格：[02 导入](../../docs/prd/02-ingest.md) · 最后更新：2026-08-13

## 一句话

用户拖入 PNG/JPEG 或提交一段口述文本后，Daybook 先保存不可变证据与来源身份，再串行触发 agent 解析。

## 数据流

```text
拖拽文件 / 提交口述
  → src/App.tsx
  → import_source_file / submit_utterance Tauri command
  → src-tauri/src/ingest.rs
  → <data>/evidence/YYYY/MM/<source_id>.<ext>
  → sources(state = imported)
  → parse_source
  → AgentRuntime
  → sources(state = parsed | failed)
```

## 关键文件

| 文件 | 职责 |
|---|---|
| `src/App.tsx` | 拖放、口述提交令牌、来源列表、失败重试与停止 |
| `src-tauri/src/ingest.rs` | magic bytes 校验、SHA-256、证据落盘、幂等、状态机、启动恢复 |
| `src-tauri/src/lib.rs` | 导入、口述、解析与取消 Tauri commands；窗口出现前恢复中断任务 |
| `src-tauri/migrations/0001_m0.sql` | `sources`、文件部分唯一索引、口述令牌唯一索引、状态约束 |

## 数据结构

- `sources.kind` 为 `file` 或 `utterance`。
- 文件保存 `content_hash`、原文件名与实际格式；口述保存 `idempotency_key`，正文落成 `.txt`。
- `latest_attempt_id` 只能指向同一来源的 attempt；跨来源关联由 trigger 拒绝。

## 业务规则

- PNG/JPEG 以 magic bytes 判定，不信任扩展名；原始字节逐位复制，不转码、不移动用户文件。
- 证据先落盘，来源行后写库；数据库写失败会清理本次孤儿证据。
- 文件按内容 SHA-256 去重且成功返回 `deduplicated: true`。口述只按一次提交令牌幂等；相同文本的新令牌仍创建新来源并提示候选。
- 未选择本位币时解析返回 `data.base_currency_required`，来源留在 `imported`，不创建 attempt。
- 解析全局串行；失败不自动重试。用户重试会创建新 attempt。
- 启动时扫描卡在 `parsing` 或未闭合的 attempt，统一收束为 `agent.interrupted` 并作废活动草稿。

## 已知边界与坑

- M0 不支持 HEIC/PDF；必须明确报 `ingest.unsupported_format`。
- `parsed` 的判据不是退出码 0，而是成功调用 `complete_source` 后正常退出。
- 批量拖入在前端逐个串行调用；单张失败不会破坏已经持久化的其他来源。

## 相关

- [Agent 运行时](./agent-runtime.md)
- [审核与确认](./review-and-confirm.md)
- [ADR-0002](../../docs/adr/0002-ai-never-writes-directly.md)
