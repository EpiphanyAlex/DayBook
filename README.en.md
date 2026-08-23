# Daybook

[中文](./README.md) · **English**

**A local-first AI personal admin assistant that turns scattered money and life details into financial records and plans — without making you fill out one form at a time.**

**Life can happen first. The record can catch up later.** Give Daybook screenshots or tell it what happened and what comes next; it separates the transactions and items into drafts for you to review.

In v1, “personal admin” means exactly two entities: transactions and items. Daybook is not a full calendar or a general-purpose secretary; its core promise is to remove the repetitive, one-record-at-a-time form work from tracking money and planning tasks.

> `daybook` is the accounting term for a day book: a chronological register of original vouchers. Read literally it is also "the book of days." One word covers both halves of the product — money and time — and it happens to describe the data model exactly: a single timeline of evidence-backed raw records, ordered by date.

> **Note on language:** the design documents in this repository are written in Chinese. This file is the English mirror of [`README.md`](./README.md). Every link below leads to a Chinese document, with one exception — [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md) is in English, and you may write the pull request body in either language.

---

## Current status

**M0 implementation is under review (2026-08-23).** Tauri / React / Rust have landed: the six-table foundation, the sealed five-tool agent path, screenshot and utterance import, review-and-confirm, and the total cross-check all run end to end; both `src/` and `src-tauri/` now exist. M0 is defined as the **end-to-end smoke test** — drop in a screenshot → the agent reads it → **drafts are written through MCP** → **a human confirms** → the fact tables are written → the list renders it. Milestone table: [`docs/PRD.md` §9](./docs/PRD.md).

**M0 is not done yet, though.** [00 Foundation](./docs/prd/00-foundation.md), [01 Agent runtime](./docs/prd/01-agent-runtime.md), [02 Ingest](./docs/prd/02-ingest.md), and [03 Review and drafts](./docs/prd/03-review.md) are all in `review` — 01's installation-eligibility and parsing-readiness contract was disproved and rewritten, and both the corrected implementation and its three on-device manual checks were completed on 2026-08-23. Neither the maintainer's manual review nor the real-sample go / no-go in [`docs/PRD.md` §9.4](./docs/PRD.md) has happened. **The current interface is a functional baseline, not an approved design** — the design and the semantic token system get settled before M1 starts. Status overview: [`docs/prd/INDEX.md`](./docs/prd/INDEX.md).

**The spike that blocked M0 was completed on 2026-08-12**: the MCP server lives in a standalone helper binary that talks back to the main process over a Unix domain socket ([`docs/prd/01-agent-runtime.md` §3.1](./docs/prd/01-agent-runtime.md); measurements in [`docs/spikes/`](./docs/spikes/)).

---

## Running it

**This is a macOS desktop app** ([ADR-0001](./docs/adr/0001-local-first-desktop-platform.md)). Prerequisites:

| Required | Why |
|---|---|
| macOS + Xcode Command Line Tools | Needed to build Tauri |
| Node.js 20.19+ / 22.12+ | Vite 7's requirement |
| Rust 1.85+ | See `rust-version` in [`src-tauri/Cargo.toml`](./src-tauri/Cargo.toml) |
| **Your own agent CLI** | Claude Code is the backend implemented today: `claude` installed **and logged in**. Daybook ships no vendor credentials and offers no third-party sign-in — it uses the subscription you already have |

```bash
npm install
npm run tauri dev
```

**Pick a base currency in the left pane before your first parse**, or parsing returns `data.base_currency_required` — Daybook will not guess it from your locale. After that, drag a screenshot into the left pane, or just type out what you remember as a sentence.

Everything persisted (ledger, original evidence files, logs) lives in the application data directory; the "reveal in Finder" button in the UI opens it.

### Gates

Seven of them, all equal — any failure is red ([`CLAUDE.md`](./CLAUDE.md) constraint 16 and「常用命令」/ common commands):

```bash
npm run lint && npm run typecheck && npm test && npm run build
cd src-tauri && cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Plus four documentation gates ([`.github/workflows/docs.yml`](./.github/workflows/docs.yml)):

```bash
node docs/prd/check-docs.mjs        # frontmatter + links under docs/prd/
node scripts/check-links.mjs        # Markdown links across the whole repo
node scripts/check-readme-sync.mjs  # README.en.md is not behind README.md
node scripts/check-spec-invariants.mjs  # no superseded conclusions left in current sections
```

One command runs all eleven of the above, plus the real-CLI capability probe and the screenshot / utterance happy paths:

```bash
node scripts/verify-m0.mjs              # includes the two real-CLI steps; spends your own quota
node scripts/verify-m0.mjs --skip-live  # skips the real CLI; this is not a full M0 pass
```

CI runs both on every PR: the documentation gates via [`docs.yml`](./.github/workflows/docs.yml), the code gates via [`ci.yml`](./.github/workflows/ci.yml) (that is, the `--skip-live` command above). **A green CI is not an M0 pass** — the two real-CLI steps need a logged-in agent CLI and can only run on your own machine.

---

## The problem

Expense trackers are designed around the premise that you record a transaction as it happens — a premise that almost never holds in practice. Recording slips, and then one day you sit down in front of a pile of vague traces and try to reconstruct the last two weeks.

**Existing tools abandon the user at exactly that step.** They keep optimizing for capture-in-the-moment and leave after-the-fact reconstruction entirely manual: scrolling statements, cross-checking screenshots, typing entries one at a time, doing exchange-rate math by hand. Catching up once costs far more than recording ten times in the moment — so people quit.

The same friction appears in planning: calendar and to-do tools ask you to create each item separately and keep filling in titles, dates, states and durations. Money and tasks live in different apps, but the burden is the same — **repeatedly translating a life that has happened, or is about to happen, into forms.**

Daybook starts from the opposite premise: **the primary path should not require one form per record; life can happen first, and the record can catch up later.** You hand over screenshots, an utterance or text, and the agent organizes the transactions and items into drafts for review.

## How it works

1. **The user brings their own AI quota → marginal cost on the product side is zero → any source can be re-parsed.**
   A conventional expense app has to ship a dedicated parser per bank: it breaks when a format changes, the long tail is never covered, and every new country means doing it all again. Competitors billing per API call end up back at hand-written parsers once they do the token math.
   **This capability is inherently bank-, currency-, and country-agnostic — because it does not recognize any specific format in the first place.** Multi-currency, multi-channel and arbitrary layouts fall out of it for free; none of them needs to be designed separately.
   *("Zero marginal cost" means on this project's side: the tokens are billed to your own subscription or API account. It is not zero on your side — quotas are finite and going over them may cost money.)*
2. **Reuse the agent CLI the user already has installed (Claude Code / Codex) → a mature agent runtime for free.**
   This is also why it **has to** be a desktop app: it needs local login state, local processes, local files.
3. **Local-first: no Daybook account, no remote server, none of your data hosted by us.**
   Ledgers and calendars are deeply private. You never register an account for Daybook, and this project runs no remote service — **so no server operated by us ever retains your data**. (Whether the model provider you chose retains anything, and for how long, is governed by *their* policy — we neither handle that nor control it; see below.)

### Where the data actually goes, stated plainly

Two separate things here — **where things are stored**, and **what leaves the machine during parsing**. Conflating them is what makes "evidence files never leave your machine" and "screenshots are sent to a model provider" look like a contradiction.

**Where it is stored (persistence)**

| | Location |
|---|---|
| Ledger, evidence files, logs | **Only on your machine** — the app data directory, visible to you, deletable by you |
| Daybook account / remote server / cloud sync / telemetry / crash reporting | **Do not exist**; this project runs no remote service |

**What leaves the machine (transmission)**

Parsing relies on an agent CLI *you* installed and logged into, and the inference behind `claude -p` / `codex exec` runs at their respective model providers. **When a screenshot is parsed, that screenshot and the related text are sent to that provider by that CLI**, using *your* subscription and *your* login with that provider. The CLI initiates this itself — Daybook does not proxy it, forward it, or log it — but it does happen. **It creates no storage on Daybook's side**: what goes out does not pass through our servers, because there are none.

**Where the parsed content goes depends on the backend you pick**: switch to a local model process ([ADR-0003](./docs/adr/0003-agent-runtime-and-pluggable-backend.md), one of the pluggable options) and the content being parsed **never has to be sent to a remote model provider**. Note that this is about the *content* — **whether that local process itself talks to the network** (update checks, usage reporting) is its own business, which Daybook neither controls nor vouches for. **This is a property of the backend you chose, not a default promise of the product.**

> **A note on the word "server"**: the "Rust MCP server" and the "agent backend" in the tech stack are both machine-local — the former is the tool surface exposed to the agent CLI, the latter is the abstraction over *which local process does the inference*. **Neither is a remote server**; do not confuse them with the "no remote server" claim above.

The pain grows with **number of accounts → number of payment channels → number of currencies**; the more of each, the more obvious the value. The validation sample is a **multi-account, multi-channel, dual-currency** setup because it puts the most pressure on parsing — **a stress test, not a market boundary.**

---

## The line that must not be crossed: AI never writes to the ledger

Vision models **really do read 168 as 1680**, and a single wrong number in a ledger destroys trust permanently. Four gates (full argument in [ADR-0002](./docs/adr/0002-ai-never-writes-directly.md)):

| Gate | What it does |
|---|---|
| **Draft area** | The AI writes only to `draft_*` tables; nothing reaches the fact tables until a human confirms it. The subprocess starts under a sealed configuration, and **the tools it can actually reach** are probed before any task is dispatched |
| **Evidence chain** | Every draft carries its origin — which screenshot (or which spoken utterance), and which passage the model claims it read — and what you get side by side when reviewing is **the original itself**, replacing "trust the AI" with "glance at the original" |
| **Total cross-check** | The entries extracted from one source must add up to the total printed on that source itself; a mismatch raises an alarm without anyone asking. When a source has no printed total by its very nature (a sentence you spoke, say), that gate is swapped for another: the full transcript shown beside every extracted entry, and your single confirming keystroke |
| **Append-only audit log** | Every AI write and every human edit leaves a trace; **the AI's original draft is kept forever** |

---

## Tech stack

| Layer | Choice | When |
|---|---|---|
| UI | React + TypeScript + Vite (major version pinned to whatever is current when [`00-foundation`](./docs/prd/00-foundation.md) scaffolds the project) | v1 |
| Desktop shell | Tauri 2 | v1 |
| Core | Rust — `rusqlite` + process management + file watching | v1 |
| Agent tool surface | Rust MCP server (`rmcp`, the official SDK) | v1 |
| Agent backend | Pluggable interface; **a backend is always an external process you already configured**: `claude -p` / `codex exec` / a local model process | Interface in v1; only Claude Code implemented in v1 |
| Photo library access | Swift sidecar (PhotoKit, headless standalone binary) | v1.1 |
| Voice | macOS system dictation in v1 (zero code) → Swift sidecar in v1.1 | v1 + v1.1 |

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
| App icon and loading animation (palette, geometry, motion spec) | [`assets/brand/README.md`](./assets/brand/README.md) |
| UI token system (color ramps, type scale, spacing, component specs) — **under review** | [`design.md`](./design.md) |

Opening a pull request? The workflow and the template are described in [`CLAUDE.md`](./CLAUDE.md).

**This project does not use tickets.** Humans write *what and why* (a sub-PRD), the agent produces *how* (plan mode), humans review the plan. Rationale and workflow: [`CLAUDE.md`](./CLAUDE.md) under「PRD 体系与工作流」(PRD system and workflow).

---

## Success criteria

- **One month in**: three focused sessions organizing both money and items, **and not one of them followed by going back to the original screenshots out of doubt about the ledger**. The moment quiet double-checking starts, it has already failed.
- **End state**: the old expense app and the old calendar app stop being opened.

---

## License

**The code**: [MIT](./LICENSE). Use it however you want — fork it, modify it, ship it in a closed-source product, sell it. No permission needed; attribution appreciated, not required.

**Your data is not covered by that license.** The ledger, the evidence screenshots, the transcribed text and the logs have never left your machine, and have never passed through any service operated by this project — so they need nobody's permission, and there is no right here for us to grant or revoke.

**The agent CLI is not part of Daybook.** Claude Code and Codex are published by their respective vendors under their own licenses and terms of service; you are using your own subscription and your own login. **Whether running Daybook's parsing through them complies with those terms is something we have not finished verifying** ([`docs/PRD.md` §12](./docs/PRD.md), [`docs/prd/01-agent-runtime.md` §5](./docs/prd/01-agent-runtime.md) R4) — it has to be settled before packaging and release in M4. Until then, please confirm for yourself that your usage fits the terms of the CLI you use.
