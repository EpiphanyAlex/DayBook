# Daybook

[中文](./README.md) · **English**

**A local-first AI desktop assistant that helps you reconstruct the money and the days you never got around to recording.**

It is not an expense tracker and not a to-do app. It is a **retroactive recorder** — built for *catching up after the fact*, not for *capturing in the moment*.

> `daybook` is the accounting term for a day book: a chronological register of original vouchers. Read literally it is also "the book of days." One word covers both halves of the product — money and time — and it happens to describe the data model exactly: a single timeline of evidence-backed raw records, ordered by date.

> **Note on language:** the design documents in this repository are written in Chinese. This file is the English mirror of [`README.md`](./README.md). Every link below leads to a Chinese document, with one exception — [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md) is in English, and you may write the pull request body in either language.

---

## Current status

**Skeleton stage (established 2026-08-06).** Constraints and documentation are in place; `src/` and `src-tauri/` **do not exist yet**.
The first milestone is **M0 — end-to-end smoke test**: drop in a screenshot → the agent reads it → it is written to SQLite through MCP → the list renders it. Milestone table: [`docs/PRD.md` §9](./docs/PRD.md).

The only commands that run today are the documentation gates, enforced by CI on every PR (see [`.github/workflows/docs.yml`](./.github/workflows/docs.yml)):

```bash
node docs/prd/check-docs.mjs        # frontmatter + links under docs/prd/
node scripts/check-links.mjs        # Markdown links across the whole repo
node scripts/check-readme-sync.mjs  # README.en.md is not behind README.md
```

The frontend and Rust commands only exist once [`docs/prd/00-foundation.md`](./docs/prd/00-foundation.md) lands; the full list is in [`CLAUDE.md`](./CLAUDE.md) under「常用命令」(common commands).

---

## The problem

Expense trackers are designed around the premise that you record a transaction as it happens — a premise that almost never holds in practice. Recording slips, and then one day you sit down in front of a pile of vague traces and try to reconstruct the last two weeks.

**Existing tools abandon the user at exactly that step.** They keep optimizing for capture-in-the-moment and leave after-the-fact reconstruction entirely manual: scrolling statements, cross-checking screenshots, typing entries one at a time, doing exchange-rate math by hand. Catching up once costs far more than recording ten times in the moment — so people quit.

Daybook starts from the opposite premise: **you are catching up, by default.**

## How it works

1. **The user brings their own AI quota → marginal cost is zero → any source can be re-parsed.**
   A conventional expense app has to ship a dedicated parser per bank: it breaks when a format changes, the long tail is never covered, and every new country means doing it all again. Competitors billing per API call end up back at hand-written parsers once they do the token math.
   **This capability is inherently bank-, currency-, and country-agnostic — because it does not recognize any specific format in the first place.** Multi-currency, multi-channel and arbitrary layouts fall out of it for free; none of them needs to be designed separately.
2. **Reuse the agent CLI the user already has installed (Claude Code / Codex) → a mature agent runtime for free.**
   This is also why it **has to** be a desktop app: it needs local login state, local processes, local files.
3. **Local-first, no accounts, no backend.**
   Ledgers and calendars are deeply private. Data never leaves the machine → privacy holds by construction, there is no per-country compliance surface, and server cost is zero.

The pain grows with **number of accounts → number of payment channels → number of currencies**; the more of each, the more obvious the value. The validation sample is a **multi-account, multi-channel, dual-currency** setup because it puts the most pressure on parsing — **a stress test, not a market boundary.**

---

## The line that must not be crossed: AI never writes to the ledger

Vision models **really do read 168 as 1680**, and a single wrong number in a ledger destroys trust permanently. Four gates (full argument in [ADR-0002](./docs/adr/0002-ai-never-writes-directly.md)):

| Gate | What it does |
|---|---|
| **Draft area** | The AI writes only to `draft_*` tables; nothing reaches the fact tables until a human confirms it |
| **Evidence chain** | Every draft carries its origin — which screenshot, which line of source text — replacing "trust the AI" with "glance at the original" |
| **Total cross-check** | The entries extracted from one source must add up to the total or balance that source itself declares; a mismatch raises an alarm without anyone asking |
| **Append-only audit log** | Every AI write and every human edit leaves a trace |

---

## Tech stack

| Layer | Choice | When |
|---|---|---|
| UI | React 18 + TypeScript + Vite | v1 |
| Desktop shell | Tauri 2 | v1 |
| Core | Rust — `rusqlite` + process management + file watching | v1 |
| Agent tool surface | Rust MCP server (`rmcp`, the official SDK) | v1 |
| Agent backend | Pluggable interface: `claude -p` / `codex exec` / API key / local model | Interface in v1; only Claude Code implemented in v1 |
| Photo library access | Swift sidecar (PhotoKit, headless standalone binary) | v1.1 |
| Voice | macOS system dictation in v1 (zero code) → Swift sidecar in v1.1 | v1.1 |

The reasoning behind these choices is in [ADR-0001](./docs/adr/0001-local-first-desktop-platform.md) (why not SwiftUI, why not Electron) and [ADR-0003](./docs/adr/0003-agent-runtime-and-pluggable-backend.md) (why a local MCP server).

---

## Documentation map

| Looking for | Read |
|---|---|
| Collaboration rules and the 17 implementation constraints | [`CLAUDE.md`](./CLAUDE.md) |
| A slimmed-down entry point for Codex | [`AGENTS.md`](./AGENTS.md) |
| Product scope, success criteria, non-goals, milestones | [`docs/PRD.md`](./docs/PRD.md) |
| Decisions that are hard to reverse | [`docs/adr/`](./docs/adr/) |
| System architecture baseline | [`docs/architecture.md`](./docs/architecture.md) |
| Terminology (transaction / item / draft / evidence / source / base currency…) | [`docs/CONTEXT.md`](./docs/CONTEXT.md) |
| Specs for individual capabilities | [`docs/prd/INDEX.md`](./docs/prd/INDEX.md) |
| Implementation rules split by topic | [`.claude/rules/`](./.claude/rules/) |
| "How is this feature actually implemented right now?" | [`.claude/features/`](./.claude/features/) |
| Dev-time subagent roster (**not** the product runtime agent) | [`.claude/agents/README.md`](./.claude/agents/README.md) |
| What a pull request must fill in (English skeleton; body in either language) | [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md) |

**This project does not use tickets.** Humans write *what and why* (a sub-PRD), the agent produces *how* (plan mode), humans review the plan. Rationale and workflow: [`CLAUDE.md`](./CLAUDE.md) under「PRD 体系与工作流」(PRD system and workflow).

---

## Success criteria

- **One month in**: three catch-up sessions done with it, **and not one of them followed by going back to the original screenshots out of doubt**. The moment quiet double-checking starts, it has already failed.
- **End state**: the old expense app and the old calendar app stop being opened.

---

## License

Released under the [MIT License](./LICENSE).

Rationale and decision record: [`docs/PRD.md` §13](./docs/PRD.md) (open question P4, closed 2026-08-09).
