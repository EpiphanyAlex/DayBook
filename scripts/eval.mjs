#!/usr/bin/env node
//
// eval 的 Node 薄壳。算分、正式 manifest 门禁、首轮 / finalize / diagnosis 协议都在
// `daybook-eval` Rust 子命令；这里仅选择模式、起进程、生成本机报告路径并渲染整数计数。
//
// 兼容契约：`node scripts/eval.mjs` 不带参数仍是 ad-hoc live（会烧额度），但它不是
// docs/PRD.md §9.4 的正式 verdict。只有 `--m0-go-no-go` 能产正式 verdict。

import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs'
import { basename, dirname, extname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const rust = join(root, 'src-tauri')
const defaultManifest = join(root, 'fixtures', 'manifest.json')

const HELP = `用法：
  node scripts/eval.mjs --help                                  零额度
  node scripts/eval.mjs --dry-run [--manifest <path>]           零额度，只校验普通 manifest
  node scripts/eval.mjs --replay [--manifest <path>]            零额度，重放夹具
  node scripts/eval.mjs [--manifest <path>] [--trials N]        烧额度，ad-hoc live；不是 M0 verdict
  node scripts/eval.mjs --m0-go-no-go --manifest <fixtures/local/.../manifest.json>
                                                               烧额度，正式首轮
  node scripts/eval.mjs --m0-finalize <first-report>            零额度，不重跑 agent
  node scripts/eval.mjs --m0-diagnose <first-report>            烧额度，每目标追加 3 轮

正式退出码：0=go/conditional-go · 1=运行/基础设施错误 · 2=incomplete · 3=no-go。
--replay 即使报告显示 no-go 也不是正式 verdict，保持普通命令退出语义。`

function optionalArgument(name) {
  const index = process.argv.indexOf(`--${name}`)
  return index === -1 ? undefined : process.argv[index + 1]
}

function failUsage(message) {
  console.error(`[eval] ${message}\n\n${HELP}`)
  process.exit(1)
}

if (process.argv.includes('--help') || process.argv.includes('-h')) {
  console.log(HELP)
  process.exit(0)
}

// 无参数 live 是刻意保留的兼容行为；反过来，任何拼错的参数都必须 fail closed，不能因
// `--m0-finalise` 之类的笔误意外落进 live 并烧额度。
const booleanFlags = new Set(['--dry-run', '--replay', '--m0-go-no-go'])
const valueFlags = new Set([
  '--manifest',
  '--trials',
  '--keep-runs',
  '--m0-finalize',
  '--m0-diagnose',
  '--out',
])
for (let index = 2; index < process.argv.length; index += 1) {
  const flag = process.argv[index]
  if (booleanFlags.has(flag)) continue
  if (!valueFlags.has(flag)) failUsage(`不认识的参数 ${flag}`)
  const value = process.argv[index + 1]
  if (!value || value.startsWith('--')) failUsage(`${flag} 缺参数值`)
  index += 1
}

const formalFirst = process.argv.includes('--m0-go-no-go')
const finalizeReport = optionalArgument('m0-finalize')
const diagnoseReport = optionalArgument('m0-diagnose')
const dryRun = process.argv.includes('--dry-run')
const replay = process.argv.includes('--replay')
const selectedModes = [formalFirst, finalizeReport !== undefined, diagnoseReport !== undefined, dryRun, replay]
  .filter(Boolean).length
if (selectedModes > 1) failUsage('模式参数互斥')
const hasAdHocTuning = process.argv.includes('--trials') || process.argv.includes('--keep-runs')
if (formalFirst && hasAdHocTuning) {
  failUsage('M0 正式首轮每 case 恰好 1 轮；三轮只经 --m0-diagnose')
}
if ((dryRun || replay || finalizeReport !== undefined || diagnoseReport !== undefined) && hasAdHocTuning) {
  failUsage('--trials / --keep-runs 只属于无模式参数的 ad-hoc live')
}
if (process.argv.includes('--out') && !formalFirst && finalizeReport === undefined && diagnoseReport === undefined) {
  failUsage('--out 只属于 M0 正式 first / final / diagnosis')
}
if (process.argv.includes('--m0-finalize') && !finalizeReport) failUsage('--m0-finalize 缺首轮报告路径')
if (process.argv.includes('--m0-diagnose') && !diagnoseReport) failUsage('--m0-diagnose 缺首轮报告路径')

const manifest = optionalArgument('manifest')
  ? resolve(root, optionalArgument('manifest'))
  : defaultManifest

// 优先用已经构建好的二进制（verify-m0.mjs 在 cargo build --bins 之后调用）；单独运行时
// 退回 cargo run。所有 cargo 调用都 offline。
function evalCommand(args) {
  const binary = join(rust, 'target', 'debug', 'daybook-eval')
  if (existsSync(binary)) return { command: binary, args, cwd: root }
  return {
    command: 'cargo',
    args: ['run', '--offline', '--quiet', '--bin', 'daybook-eval', '--', ...args],
    cwd: rust,
  }
}

function runEval(args, { capture = false, allowedStatuses = [0] } = {}) {
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
  const status = result.status ?? 1
  if (!allowedStatuses.includes(status)) process.exit(status)
  return { status, stdout: result.stdout }
}

if (dryRun) {
  runEval(['validate', '--manifest', manifest, '--root', root])
  process.exit(0)
}

function localReportPath(kind) {
  const directory = join(root, 'output', 'm0-eval')
  mkdirSync(directory, { recursive: true })
  const stamp = new Date().toISOString().replaceAll(':', '').replaceAll('.', '-')
  return join(directory, `${stamp}-${kind}.json`)
}

function siblingWithSuffix(path, suffix) {
  const extension = extname(path)
  const stem = basename(path, extension)
  return join(dirname(path), `${stem}${suffix}`)
}

let reportPath
let commandStatus = 0
let reportKind = 'evaluation'

if (replay) {
  reportPath = join(rust, 'target', 'eval-report.json')
  runEval(['replay-score', '--manifest', manifest, '--root', root, '--out', reportPath])
} else if (formalFirst) {
  if (!optionalArgument('manifest')) failUsage('--m0-go-no-go 必须显式给 --manifest <fixtures/local/...>')
  reportPath = optionalArgument('out')
    ? resolve(root, optionalArgument('out'))
    : localReportPath('first')
  const result = runEval(
    ['m0-go-no-go', '--manifest', manifest, '--root', root, '--out', reportPath],
    { allowedStatuses: [0, 2, 3] },
  )
  commandStatus = result.status
} else if (finalizeReport !== undefined) {
  const first = resolve(root, finalizeReport)
  reportPath = optionalArgument('out')
    ? resolve(root, optionalArgument('out'))
    : siblingWithSuffix(first, '.final.json')
  const result = runEval(['m0-finalize', '--report', first, '--out', reportPath], {
    allowedStatuses: [0, 2, 3],
  })
  commandStatus = result.status
  if (commandStatus === 2 && !existsSync(reportPath)) process.exit(2)
} else if (diagnoseReport !== undefined) {
  const first = resolve(root, diagnoseReport)
  reportPath = optionalArgument('out')
    ? resolve(root, optionalArgument('out'))
    : siblingWithSuffix(first, `.diagnosis.${new Date().toISOString().replaceAll(':', '').replaceAll('.', '-')}.json`)
  runEval(['m0-diagnose', '--report', first, '--root', root, '--out', reportPath])
  reportKind = 'diagnosis'
} else {
  // **兼容的 ad-hoc live，会烧额度**。它真起用户自己的 agent CLI，但不产正式 M0 verdict。
  reportPath = join(rust, 'target', 'eval-report.json')
  const passthrough = ['run', '--manifest', manifest, '--root', root, '--out', reportPath]
  for (const name of ['trials', 'keep-runs']) {
    const value = optionalArgument(name)
    if (value !== undefined) passthrough.push(`--${name}`, value)
  }
  runEval(passthrough)
}

const stored = JSON.parse(readFileSync(reportPath, 'utf8'))
if (reportKind === 'diagnosis' || stored.stage === 'diagnosis') {
  console.log(`\nM0 三轮诊断（独立报告，不覆盖首轮）· 每 case 追加 ${stored.roundsPerCase} 轮`)
  for (const item of stored.cases) {
    const rounds = item.rounds
      .map((round) => `第${round.round}轮${round.passed ? '过' : '不过'}${round.executionError ? `(${round.executionError})` : ''}`)
      .join(' · ')
    console.log(`  ${item.id}: ${item.verdict} · ${rounds}`)
  }
  console.log(`报告：${reportPath}\n`)
  process.exit(0)
}

const formal = stored.mode === 'm0_go_no_go' && stored.evaluation
const report = stored.evaluation ?? stored
if (!formal && (replay || !formalFirst) && reportPath.includes(join('target', 'eval-report.json'))) {
  rmSync(reportPath, { force: true })
}

// **每个比率一律连原始计数一起报**（docs/PRD.md §9.4）。
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
  screenshot: '截图池（判定：指标 1–3）',
  utterance: '口述池（判定：指标 1–3）',
  control: '对照栏（只记录，不参与 go / no-go）',
}

const MODE_TEXT = { replay: '零额度重放', live: '真跑 agent，烧订阅额度' }
console.log(`\n模式：${formal ? `${stored.stage} M0 正式判定` : `${report.mode}（${MODE_TEXT[report.mode] ?? report.mode}）`}`)
if (!formal && report.mode === 'live') console.log('注意：这是 ad-hoc live，不是 docs/PRD.md §9.4 的正式 verdict。')
if (report.trials > 1) {
  console.log(`--trials ${report.trials}：仅属 ad-hoc 诊断，不覆盖第 1 轮`)
}
console.log(`阈值与口径出处：${report.thresholdsSource ?? report.thresholds_source}\n`)

console.log('逐条')
console.log('  用例                 池          匹配 漏读 多读  对账        后端/模型                prompt_hash')
for (const item of report.cases) {
  const attribution = `${item.attribution.backendId}/${item.attribution.modelId ?? '—'}`
  console.log(
    `  ${item.id.padEnd(20)} ${item.pool.padEnd(11)}` +
      `${String(item.matched).padStart(3)} ${String(item.missed).padStart(4)} ${String(item.extra).padStart(4)}` +
      `  ${item.reconciliationStatus.padEnd(11)} ${attribution.padEnd(24)} ${item.attribution.promptHash.slice(0, 12)}`,
  )
  if (item.executionError) console.log(`      ↳ case 质量失败（已记录并继续）：${item.executionError}`)
  for (const pair of item.join.matched) {
    if (pair.wrongFields.length) console.log(`      ↳ 第 ${pair.sourceOrdinal} 条字段错：${pair.wrongFields.join(' · ')}`)
  }
  for (const ordinal of item.join.missed) console.log(`      ↳ 漏读 第 ${ordinal} 条`)
  for (const ordinal of item.join.extra) console.log(`      ↳ 多读 第 ${ordinal} 条`)
  if (item.unparsedNote) console.log(`      ↳ agent 自述未解析：${item.unparsedNote}`)
  if (item.trialDiagnostics) {
    const TRIAL_TEXT = { all_passed: '全过', mixed: '部分过', none_passed: '全不过' }
    const rounds = item.trialDiagnostics.passedPerTrial
      .map((passed, index) => `第${index + 1}轮${passed ? '过' : '不过'}`)
      .join(' · ')
    console.log(`      ↳ [${item.trialDiagnostics.label}] ${item.trialDiagnostics.trials} 轮${TRIAL_TEXT[item.trialDiagnostics.verdict]}：${rounds}`)
  }
  for (const id of item.substringViolations) console.log(`      ↳ 抽取声明不是转写文本的子串：${id}`)
}

for (const pool of report.pools) {
  console.log(`\n${POOL_TITLE[pool.pool] ?? pool.pool} · ${pool.caseCount} 条来源`)
  for (const metric of pool.metrics) {
    console.log(
      `  ${VERDICT_MARK[metric.verdict] ?? ' '} ${String(metric.index).padStart(2)}. ` +
        `${metric.label.padEnd(38)} ${formatRatio(metric.ratio).padStart(16)}  ${THRESHOLD_TEXT(metric.threshold)}`,
    )
  }
  const { matched, expectedOnly, predictedOnly } = pool.degraded
  console.log(`  [${pool.degraded.label}] 集合匹配：对上 ${matched} · 只在期望侧 ${expectedOnly} · 只在草稿侧 ${predictedOnly}`)
}

console.log('\n正式判定集合（截图池 + 口述池）· 指标 4–8')
for (const metric of report.decisionMetrics) {
  console.log(
    `  ${VERDICT_MARK[metric.verdict] ?? ' '} ${String(metric.index).padStart(2)}. ` +
      `${metric.label.padEnd(38)} ${formatRatio(metric.ratio).padStart(16)}  ${THRESHOLD_TEXT(metric.threshold)}`,
  )
}

console.log('\n只记录 · 指标 9–10（不进入 verdict）')
for (const item of report.cases) {
  const duration = item.durationMs === null ? '—' : `${item.durationMs} ms`
  const usage = item.usage === null ? 'usage: —' : `usage: ${JSON.stringify(item.usage)}`
  console.log(`  ${item.id}: ${duration} · ${usage}`)
}

const verdict = formal ? stored.verdict : report.verdict
console.log(`\n判定：${verdict}`)
if (formal) {
  console.log(`状态：${stored.status} · exit ${stored.exitCode} · 报告：${reportPath}`)
  if (stored.adjudicationsFile) console.log(`指标 5 裁定：${join(dirname(reportPath), stored.adjudicationsFile)}`)
} else if (report.mode === 'replay') {
  console.log('注：这是夹具重放，不是 docs/PRD.md §9.4 的真实样本 go / no-go。')
} else {
  console.log('注：ad-hoc live 只供观察；正式 M0 判定必须显式使用 --m0-go-no-go。')
}
console.log('')
process.exit(commandStatus)
