<!--
This template is in English because the repository is public and a PR is the first
thing an outside contributor fills in. Write the body itself in whichever language
you think in — Chinese and English are both fine. The project's design documents
are Chinese, and Chinese remains the source of truth (see CLAUDE.md「文档层级」).
-->

## What does this PR do?

<!-- One sentence. -->

## Which sub-PRD

<!-- Link docs/prd/NN-xxx.md. Write "none" for docs-only PRs. -->

- Spec: `docs/prd/`
- Which part of that sub-PRD this PR covers:

## Type

- [ ] `feat` — New feature
- [ ] `fix` — Bug fix
- [ ] `refactor` — Restructuring (no behaviour change)
- [ ] `docs` — Documentation / ADR / sub-PRD
- [ ] `test` — Tests
- [ ] `chore` — Build, dependencies, tooling

## Changes

<!-- What changed, and why. -->

-

## Constraint check

<!--
Daybook's 17 non-negotiable constraints (CLAUDE.md at the repo root — Chinese, authoritative).
Tick the ones this PR touches; mark the rest N/A. **Read each one before ticking** —
this section is the reason the template exists.
-->

**Platform and privacy**

- [ ] **1 Platform** — Tauri v2 + React/TS + Rust only; no Electron, no embedded Node service, no `localhost` HTTP API (otherwise a new ADR is required — see [ADR-0001](../docs/adr/0001-local-first-desktop-platform.md))
- [ ] **2 Data never leaves the machine** — no cloud service, backend API, accounts, telemetry, crash reporting, or third-party analytics; any new dependency has been confirmed not to make requests at runtime

**AI boundary ([ADR-0002](../docs/adr/0002-ai-never-writes-directly.md))**

- [ ] **3 AI only writes drafts** — the agent has no reachable write path into the fact tables; `domain::confirm` is not called by any MCP tool
- [ ] **4 Evidence chain** — every draft carries a non-null `source_id` + source excerpt; the review screen shows the original next to the parsed result
- [ ] **5 Total cross-check** — a mismatch raises an alert and blocks batch commit; **no force / ignore bypass**
- [ ] **9 Tool permissions enforced in code** — no general-purpose "run arbitrary SQL" / arbitrary file write / arbitrary command execution tool
- [ ] **10 One agent, many tools** — agents are not split by business domain; sub-agents are used only for context isolation
- [ ] **15 Control flow decided in code** — the state machine, confirmation points, and retry policy are deterministic; the LLM makes no final business decision

**Money and data ([ADR-0004](../docs/adr/0004-data-model-sqlite-integer-money.md))**

- [ ] **6 Integer money** — integer minor units end to end; no floats in intermediate calculations or over IPC
- [ ] **7 Multi-currency triple** — original amount + base-currency amount + the rate at the time, all three present and mutually consistent
- [ ] **8 Append-only audit** — no `UPDATE` / `DELETE` against `audit_log`; both agent writes and human edits leave a trail

**Other**

- [ ] **11 Pluggable agent backend** — upper layers depend only on the interface; **no vendor credentials / endpoints / login flows in the code**
- [ ] **12 Weak-signal collection off by default** — per-item consent, revocable at any time, sensitive data flagged in the UI (mark N/A if v1 does not include this)
- [ ] **13 Voice stays local** — audio never leaves the machine; v1 uses macOS system dictation, no Swift code ([ADR-0005](../docs/adr/0005-voice-and-system-integration.md))
- [ ] **14 Memory stores rules, not conversations** — no raw conversation history is persisted
- [ ] **17 v1 scope discipline** — nothing from the non-goals in [`docs/PRD.md` §6](../docs/PRD.md) has been introduced; if scope grew, this PR updates `docs/PRD.md` too
- [ ] No new ADR needed, **or** a new/revised ADR is included in this PR

## Gates ([`CLAUDE.md`](../CLAUDE.md) constraint 16 — any failure is red)

<!-- Code PRs must run and tick these; a docs-only PR needs the doc gates only. -->

- [ ] Docs: `node docs/prd/check-docs.mjs` green (required whenever `docs/prd/` changed)
- [ ] Docs: `node scripts/check-links.mjs` green (required whenever any `.md` changed; CI enforces it on every PR)
- [ ] Docs: `node scripts/check-readme-sync.mjs` green — **if you changed [`README.md`](../README.md), you must update [`README.en.md`](../README.en.md) in the same commit**; Chinese is the source of truth, English is its mirror
- [ ] Frontend: `npm run lint` · `npm run typecheck` · `npm test` · `npm run build`
- [ ] Rust: `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test`
- [ ] Runs locally: `npm run tauri dev`

## Three closing steps ([`CLAUDE.md`](../CLAUDE.md) — miss one and the work counts as unfinished)

- [ ] **Write-back** — deviations / clarifications / new findings relative to the spec are recorded in the matching sub-PRD's「回流记录」(write-back log), version bumped by +0.1
      *(When the implementation disproves the spec, update the document first, then change the code. Plans are disposable; decisions get written back.)*
- [ ] **Status synced** — the sub-PRD's frontmatter `status` matches [`docs/prd/INDEX.md`](../docs/prd/INDEX.md)
- [ ] **Feature cheat-sheet** — a file exists under [`.claude/features/`](../.claude/features/) for a newly landed capability; later changes are reflected there

## Evidence

<!--
Whatever command the sub-PRD's acceptance criteria name, paste that command's output —
not "the tests pass", the actual output.
Same rationale as the product itself: replace "do you believe the agent says it's done?"
with "just run it and see."
UI changes: before/after screenshots.
-->

```
$ cargo test foundation::
...
```
