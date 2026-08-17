#!/usr/bin/env node
//
// eval 的薄壳（docs/prd/07-eval.md §3.4–§3.5）。
//
// **算分在 Rust 里**（`src-tauri/src/eval/`，入口 `daybook-eval`），这里只负责起进程和
// 把结果渲染成一张逐条 diff 表。分工的理由是两边改动的频率不一样：口径最不该动，报表
// 最常动；放在一个文件里迟早会一起改。
//
// 用法：
//   node scripts/eval.mjs --dry-run              不调用 agent，只校验 eval 集完整性（零额度）
//   node scripts/eval.mjs --replay               重放 fixtures/ 里的夹具并出 diff 表（零额度）
//   node scripts/eval.mjs                        **真实 eval 轮次 —— 烧订阅额度**
//   node scripts/eval.mjs --trials 3             flaky 用例跑 3 轮，报「全过 / 部分过 / 全不过」
//   node scripts/eval.mjs --keep-runs <dir>      留下每轮的数据目录，供 export-fixture 取用
//
// **本脚本不进 ci.yml。** CI 只经 verify-m0.mjs 跑到 --dry-run 那一步；真跑 agent 的轮次
// 烧订阅额度，且 CI 环境没有已登录的 agent CLI（07 §3.1 与 §4）。

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const rust = join(root, 'src-tauri')
const manifest = join(root, 'fixtures', 'manifest.json')

const mode = process.argv.includes('--dry-run')
  ? 'dry-run'
  : process.argv.includes('--replay')
    ? 'replay'
    : 'live'

function optionalArgument(name) {
  const index = process.argv.indexOf(`--${name}`)
  return index === -1 ? undefined : process.argv[index + 1]
}

// 优先用已经构建好的二进制（verify-m0.mjs 在 `cargo build --bins` 之后调用本脚本，
// 那时它一定在）；单独跑本脚本时退回 `cargo run`。
function evalCommand(args) {
  const binary = join(rust, 'target', 'debug', 'daybook-eval')
  if (existsSync(binary)) return { command: binary, args, cwd: root }
  return {
    command: 'cargo',
    args: ['run', '--offline', '--quiet', '--bin', 'daybook-eval', '--', ...args],
    cwd: rust,
  }
}

function runEval(args, { capture = false } = {}) {
  const { command, args: full, cwd } = evalCommand(args)
  const result = spawnSync(command, full, {
    cwd,
    encoding: 'utf8',
    stdio: capture ? ['inherit', 'pipe', 'inherit'] : 'inherit',
  })
  if (result.error) {
    console.error(`[eval] 无法启动 daybook-eval：${result.error.message}`)
    process.exit(1)
  }
  if (result.status !== 0) process.exit(result.status ?? 1)
  return result.stdout
}

if (mode === 'dry-run') {
  runEval(['validate', '--manifest', manifest, '--root', root])
  process.exit(0)
}

// ── 跑一轮（重放或真跑）并渲染 ────────────────────────────────────────────
const reportPath = join(rust, 'target', 'eval-report.json')
if (mode === 'replay') {
  runEval(['replay-score', '--manifest', manifest, '--root', root, '--out', reportPath])
} else {
  // **这条会烧订阅额度**：它真起用户自己的 agent CLI，一条用例 ≈ 一次真实导入（07 §3.1）。
  // 检测不到可用 CLI 时 daybook-eval 会在跑任何一条用例之前非零退出（§6）。
  const passthrough = ['run', '--manifest', manifest, '--root', root, '--out', reportPath]
  for (const name of ['trials', 'keep-runs']) {
    const value = optionalArgument(name)
    if (value !== undefined) passthrough.push(`--${name}`, value)
  }
  runEval(passthrough)
}
const report = JSON.parse(readFileSync(reportPath, 'utf8'))
rmSync(reportPath, { force: true })

// **每个比率一律连原始计数一起报**（docs/PRD.md §9.4）：`0.967 (58/60)`。小分母上的比率
// 是量化的 —— 60 条上 ≥ 0.98 实际等于「最多漏 1 条」，漏 2 条直接掉到 0.967。判定仍机械
// 按阈值走，但报告必须让「差一条」和「差五条」一眼可辨。
function formatRatio({ num, den }) {
  if (num === null) return `—      (?/${den})`
  if (den === 0) return `—      (0/0)`
  const scaled = Math.round((num / den) * 1000) / 1000
  return `${scaled.toFixed(3)} (${num}/${den})`
}

const VERDICT_MARK = {
  pass: '✓',
  fail: '✗',
  no_sample: '·',
  pending_manual: '?',
  record_only: '–',
}

const THRESHOLD_TEXT = (threshold) => {
  if (threshold === 'record_only') return '仅记录'
  if ('at_least' in threshold) return `≥ ${(threshold.at_least / 1000).toFixed(2)}`
  if ('at_most' in threshold) return `≤ ${(threshold.at_most / 1000).toFixed(2)}`
  return ''
}

const POOL_TITLE = {
  screenshot: '截图池（判定）',
  utterance: '口述池（判定）',
  control: '对照栏（只记录，不参与 go / no-go）',
}

const MODE_TEXT = { replay: '零额度重放', live: '真跑 agent，**烧订阅额度**' }
console.log(`\n模式：${report.mode}（${MODE_TEXT[report.mode] ?? report.mode}）· 每条 1 轮出正式数`)
if (report.trials > 1) {
  console.log(`--trials ${report.trials}：仅对标 flaky 的用例多跑，结果只进诊断栏，不覆盖正式数`)
}
console.log(`阈值与口径出处：${report.thresholds_source ?? report.thresholdsSource}\n`)

// ── 逐条 diff 表 ──────────────────────────────────────────────────────────
// 07 §3.5：「eval 脚本输出的是一张 diff 表（哪条变了、变成什么），不是一个分数。」
// 同时带模型标识与后端标识 —— 否则无法区分模型退步与提示词变更导致的回归。
console.log('逐条')
console.log(
  '  用例                 池          匹配 漏读 多读  对账        后端/模型                prompt_hash',
)
for (const item of report.cases) {
  const attribution = `${item.attribution.backendId}/${item.attribution.modelId ?? '—'}`
  console.log(
    `  ${item.id.padEnd(20)} ${item.pool.padEnd(11)}` +
      `${String(item.matched).padStart(3)} ${String(item.missed).padStart(4)} ${String(item.extra).padStart(4)}` +
      `  ${item.reconciliationStatus.padEnd(11)} ${attribution.padEnd(24)} ${item.attribution.promptHash.slice(0, 12)}`,
  )
  for (const pair of item.join.matched) {
    if (pair.wrongFields.length === 0) continue
    console.log(`      ↳ 第 ${pair.sourceOrdinal} 条字段错：${pair.wrongFields.join(' · ')}`)
  }
  for (const ordinal of item.join.missed) console.log(`      ↳ 漏读 第 ${ordinal} 条`)
  for (const ordinal of item.join.extra) console.log(`      ↳ 多读 第 ${ordinal} 条`)
  if (item.unparsedNote) console.log(`      ↳ agent 自述未解析：${item.unparsedNote}`)
  if (item.trialDiagnostics) {
    // 07 §3.4：报「全过 / 部分过 / 全不过」，**不取平均**。
    const TRIAL_TEXT = { all_passed: '全过', mixed: '部分过', none_passed: '全不过' }
    const rounds = item.trialDiagnostics.passedPerTrial
      .map((passed, index) => `第${index + 1}轮${passed ? '过' : '不过'}`)
      .join(' · ')
    console.log(
      `      ↳ [${item.trialDiagnostics.label}] ${item.trialDiagnostics.trials} 轮` +
        `${TRIAL_TEXT[item.trialDiagnostics.verdict]}：${rounds}`,
    )
  }
  for (const id of item.substringViolations) {
    console.log(`      ↳ 抽取声明不是转写文本的子串：${id}`)
  }
}

// ── 分池指标 ──────────────────────────────────────────────────────────────
// 07 §3.4 口径①：截图池与口述池分开报指标 1–3，两池各自带阈值判定。
for (const pool of report.pools) {
  console.log(`\n${POOL_TITLE[pool.pool] ?? pool.pool} · ${pool.caseCount} 条来源`)
  for (const metric of pool.metrics) {
    console.log(
      `  ${VERDICT_MARK[metric.verdict] ?? ' '} ${String(metric.index).padStart(2)}. ` +
        `${metric.label.padEnd(38)} ${formatRatio(metric.ratio).padStart(16)}  ${THRESHOLD_TEXT(metric.threshold)}`,
    )
  }
  // 降级的集合匹配单独成栏并标注「诊断用」—— 一份报告里同时存在两套口径而不写明哪套
  // 算数，比只有一套差的口径更危险（07 §3.2）。
  const { matched, expectedOnly, predictedOnly } = pool.degraded
  console.log(
    `  [${pool.degraded.label}] 按（日期, 金额, 币种）集合匹配：` +
      `对上 ${matched} · 只在期望侧 ${expectedOnly} · 只在草稿侧 ${predictedOnly}`,
  )
  if (matched > 0 && pool.metrics[0].ratio.num === 0) {
    console.log('  [诊断用] 内容对得上而 ordinal 对不上 ⇒ agent 报位置不可靠（07 §5 R9）')
  }
}

const VERDICT_TEXT = {
  go: 'go',
  conditional_go: 'conditional-go —— 允许进 M1，但必须记下是哪一条、对策是什么',
  no_go: 'no-go —— 指标 1–3 有一条不达标，那是产品可信性的地板',
}
console.log(`\n判定：${VERDICT_TEXT[report.verdict] ?? report.verdict}`)
if (report.mode === 'replay') {
  console.log(
    '注：这是夹具重放的结果，**不是** docs/PRD.md §9.4 的真实样本 go / no-go——' +
      '重放测的是「agent 读错时闸门有没有拦住」，不是模型读得准不准。\n',
  )
} else {
  console.log(
    '注：按 docs/PRD.md §9.4 的四步流程，这个结果**无论好坏都要记进 07 评测的回流**，' +
      '不因为「阈值定高了」而作废；要改阈值先写清为什么，并用第二批独立样本验证。\n',
  )
}
