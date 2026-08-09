<!--
This template is in English because the repository is public and a PR is the first
thing an outside contributor fills in. Write the body itself in whichever language
you think in — Chinese and English are both fine. The project's design documents
are Chinese, and Chinese remains the source of truth (see CLAUDE.md「文档层级」).
-->

## What and why

<!-- One or two sentences. What changed, and what problem it solves. -->

## Spec

<!--
Link the sub-PRD this implements (docs/prd/NN-xxx.md) and say which part of it this
PR covers. Write "docs only" or "none" if there isn't one.
-->

-

## Constraints

<!--
CLAUDE.md at the repo root lists 17 non-negotiable constraints (Chinese, authoritative).
Do NOT tick a box list here — instead, name the ones this PR actually touches and say
in one line how each is satisfied. If the answer is "none of them", say that.

Money and data changes almost always touch 3–8 (drafts-only writes, evidence chain,
total cross-check, integer minor units, currency triple, append-only audit).
Anything spawning processes or making requests touches 1–2 and 11.
-->

-

## Evidence

<!--
Paste the actual output of whatever command the sub-PRD's acceptance criteria name —
not "the tests pass". Same rationale as the product itself: replace "do you believe
the agent says it's done?" with "just run it and see."

Gates (CLAUDE.md constraint 16 — any failure is red):
  npm run lint && npm run typecheck && npm test && npm run build
  cargo fmt --all -- --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test
Docs gates (CI enforces all three on every PR):
  node docs/prd/check-docs.mjs
  node scripts/check-links.mjs
  node scripts/check-readme-sync.mjs
UI changes: before/after screenshots.
-->

```
$
```

## Docs

<!--
The three closing steps from CLAUDE.md — miss one and the work counts as unfinished.
Say what you did for each, or why it does not apply.

1. Write-back — deviations / clarifications / new findings recorded in the sub-PRD's
   「回流记录」, version bumped by +0.1. When the implementation disproves the spec,
   update the document FIRST, then change the code.
2. Status — the sub-PRD's frontmatter `status` matches docs/prd/INDEX.md.
3. Feature cheat-sheet — a file under .claude/features/ for a newly landed capability.

Also: if you touched README.md, README.en.md must catch up before this PR merges.
Putting both in one commit is the least painful way — otherwise the commit in
between is red.
-->

-
