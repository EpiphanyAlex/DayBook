#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const rust = join(root, 'src-tauri')
const skipLive = process.argv.includes('--skip-live')

function run(label, command, args, options = {}) {
  process.stdout.write(`\n[M0] ${label}\n`)
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? root,
    env: { ...process.env, ...options.env },
    encoding: 'utf8',
    stdio: 'inherit',
  })
  if (result.error) {
    console.error(`[M0] ${label} 无法启动：${result.error.message}`)
    process.exit(1)
  }
  if (result.status !== 0) {
    console.error(`[M0] ${label} 失败（exit ${result.status ?? 'signal'}）`)
    process.exit(result.status ?? 1)
  }
}

function noMatches(label, pattern, paths) {
  process.stdout.write(`\n[M0] ${label}\n`)
  const result = spawnSync('rg', ['-n', pattern, ...paths], {
    cwd: root,
    encoding: 'utf8',
  })
  if (result.error) {
    console.error(`[M0] ${label} 无法启动：${result.error.message}`)
    process.exit(1)
  }
  if (result.status === 0) {
    process.stderr.write(result.stdout)
    console.error(`[M0] ${label} 发现禁用模式`)
    process.exit(1)
  }
  if (result.status !== 1) {
    process.stderr.write(result.stderr)
    process.exit(result.status ?? 1)
  }
}

const knownPrdStatuses = new Set([
  'draft',
  'ready',
  'in-progress',
  'review',
  'done',
  'blocked',
  'archived',
])
const implementedPrdStatuses = new Set(['in-progress', 'review', 'done'])

function prdStatus(markdown, path) {
  const frontmatter = markdown.match(/^---\r?\n([\s\S]*?)\r?\n---/)?.[1]
  const status = frontmatter?.match(/^status:\s*([^\s#]+)\s*$/m)?.[1]
  if (!status || !knownPrdStatuses.has(status)) {
    console.error(`[M0] ${path} 缺少合法的 frontmatter status，无法判断是否应核对验收选择器`)
    process.exit(1)
  }
  return status
}

function verifyAcceptanceSelectors() {
  process.stdout.write('\n[M0] 已进入实施的 sub-PRD 验收选择器都命中真实测试\n')
  const listed = spawnSync('cargo', ['test', '--offline', '--', '--list'], {
    cwd: rust,
    encoding: 'utf8',
  })
  if (listed.error || listed.status !== 0) {
    process.stderr.write(listed.stderr ?? '')
    console.error(`[M0] 无法列出 Rust 测试：${listed.error?.message ?? listed.status}`)
    process.exit(1)
  }
  const docs = [
    'docs/prd/00-foundation.md',
    'docs/prd/01-agent-runtime.md',
    'docs/prd/02-ingest.md',
    'docs/prd/03-review.md',
    // 07 评测的 §6 也点名了一批 `cargo test eval::…` 选择器，而它们此前无人强制——
    // 只要有一条改了名或没落地，这里就红。它不属于 M0 四份，但它的回归夹具是零额度、
    // 进 CI 的那一半（07 §3.6 的成本表），和 M0 门禁跑在同一条命令里。
    'docs/prd/07-eval.md',
  ]
  const missing = []
  const enforced = []
  const skipped = []
  for (const path of docs) {
    const full = readFileSync(join(root, path), 'utf8')
    const status = prdStatus(full, path)
    if (!implementedPrdStatuses.has(status)) {
      skipped.push(`${path} (${status})`)
      continue
    }
    enforced.push(`${path} (${status})`)
    let section = full.match(/\n## 6\.[\s\S]*?(?=\n## 7\.)/)?.[0] ?? ''
    section = section.split('\n#### M1 必过')[0]
    for (const line of section.split('\n')) {
      if (/\*\*M[23]\*\*/.test(line)) continue
      for (const match of line.matchAll(/`cargo test(?: --offline)? ([^`]+)`/g)) {
        const selector = match[1].trim()
        if (selector && !listed.stdout.includes(selector)) {
          missing.push(`${path}: cargo test ${selector}`)
        }
      }
      for (const match of line.matchAll(/`npm test -- ([^`]+)`/g)) {
        const selector = match[1].trim()
        const found = spawnSync('rg', ['-l', '--fixed-strings', selector, 'src'], {
          cwd: root,
          encoding: 'utf8',
        })
        if (found.status !== 0) missing.push(`${path}: npm test -- ${selector}`)
      }
    }
  }
  if (!enforced.length) {
    console.error('[M0] 没有任何已进入实施的 sub-PRD；拒绝静默跳过全部验收选择器')
    process.exit(1)
  }
  if (skipped.length) {
    console.log(`[M0] 跳过尚未承诺已有实现的 sub-PRD：${skipped.join('、')}`)
  }
  if (missing.length) {
    console.error(`[M0] 以下验收选择器会匹配 0 个测试：\n${missing.join('\n')}`)
    process.exit(1)
  }
  console.log(`[M0] 已核对：${enforced.join('、')}`)
}

run('sub-PRD 格式与状态', 'node', ['docs/prd/check-docs.mjs'])
run('Markdown 链接', 'node', ['scripts/check-links.mjs'])
run('跨规格不变量', 'node', ['scripts/check-spec-invariants.mjs'])
run('README 中英文同步', 'node', ['scripts/check-readme-sync.mjs'])
run('前端 lint', 'npm', ['run', 'lint'])
run('前端类型', 'npm', ['run', 'typecheck'])
run('前端测试', 'npm', ['test'])
run('前端构建', 'npm', ['run', 'build'])
run('Rust 格式', 'cargo', ['fmt', '--all', '--', '--check'], { cwd: rust })
run('Rust clippy', 'cargo', ['clippy', '--offline', '--all-targets', '--all-features', '--', '-D', 'warnings'], { cwd: rust })
run('Rust 单元与确定性端到端', 'cargo', ['test', '--offline'], { cwd: rust })
verifyAcceptanceSelectors()
run('MCP helper 构建', 'cargo', ['build', '--offline', '--bins'], { cwd: rust })

// 07 评测 §6：不调用 agent 的情况下校验 eval 集完整性。零额度、确定性，所以进 CI；
// **真跑 agent 的 eval 轮次不在这里**，那一条烧订阅额度（07 §3.1 与 §4）。
// 放在 `--bins` 之后，是为了让 eval.mjs 直接用刚构建好的 daybook-eval，不再触发一次编译。
run('eval 集完整性（不调用 agent）', 'node', ['scripts/eval.mjs', '--dry-run'])

noMatches('MCP 不依赖提示词目录', 'prompts/', ['src-tauri/src/mcp'])
noMatches('MCP 不可达确认动作', 'confirm', ['src-tauri/src/mcp'])
noMatches('应用不打包厂商凭证', 'sk-|api[_-]?key|Authorization', ['src-tauri/src'])
noMatches('金额代码无浮点类型', 'f32|f64', ['src-tauri/src'])
noMatches('生产代码不改写 drafted_json', 'UPDATE[^\n]*drafted_json', ['src-tauri/src'])
noMatches('生产代码不修改或删除审计', 'UPDATE\\s+audit_log|DELETE\\s+FROM\\s+audit_log', ['src-tauri/src'])

run(
  '外部进程 → stdio MCP → UDS → SQLite',
  'cargo',
  ['test', '--offline', '--test', 'm0_external', '--', '--ignored', '--nocapture'],
  { cwd: rust },
)

if (skipLive) {
  console.warn('\n[M0] 已按显式 --skip-live 跳过真实 CLI；这不是完整 M0 通过。')
  process.exit(0)
}

const helper = join(rust, 'target', 'debug', 'daybook-mcp')
if (!existsSync(helper)) {
  console.error(`[M0] 找不到 MCP helper：${helper}`)
  process.exit(1)
}
const liveEnv = { DAYBOOK_MCP_HELPER: helper, DISABLE_AUTOUPDATER: '1' }
run('真实 Claude Code 可用', 'claude', ['--version'])
run(
  '真实 CLI 有效能力严格等于五工具注册表',
  'cargo',
  ['test', '--offline', 'agent::effective_tool_surface_equals_registry', '--', '--ignored', '--nocapture'],
  { cwd: rust, env: liveEnv },
)
run(
  '真实 CLI 截图与口述 happy path',
  'cargo',
  ['test', '--offline', 'real_cli_screenshot_and_utterance_happy_paths', '--', '--ignored', '--nocapture'],
  { cwd: rust, env: liveEnv },
)

console.log('\n[M0] 全部通过：确定性链路、外部 MCP 链路与真实 CLI 集成均为绿色。')
