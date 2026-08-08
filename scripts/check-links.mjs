#!/usr/bin/env node
// 全仓库 Markdown 相对链接可达性检查。
// 用法（仓库任意位置）：node scripts/check-links.mjs
//
// 分工：docs/prd/check-docs.mjs 只覆盖 docs/prd/（frontmatter + 链接）；
//      本脚本覆盖仓库全部 .md——ADR、总 PRD、CLAUDE.md、AGENTS.md、.claude/rules 等。
//      docs/prd/ 被两个脚本重复覆盖，链接规则刻意保持逐条一致，重复检查恒等价。
//
// 规则（与 docs/prd/CLAUDE.md 硬规则 3「路径真实」一致）：
//   - 同一行含「待建」的豁免——规划中尚未创建的路径按纪律就该这么标
//   - http/https/mailto 外链跳过（可达性不由本仓库负责）
//   - 纯 #锚点不查（中文标题 slug 规则易误报，与 check-docs.mjs 的已知局限相同）
//   - file:// 绝对路径直接报错：它在别人机器上永远失效，且往往泄露本机目录结构
//
// CI-ready：靠退出码报成败，非零即失败。
import { readdirSync, readFileSync, existsSync } from 'node:fs'
import { dirname, join, resolve, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'dist', '.vite'])
const problems = []

function* mdFiles(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (SKIP_DIRS.has(e.name)) continue
    const p = join(dir, e.name)
    if (e.isDirectory()) yield* mdFiles(p)
    else if (e.name.endsWith('.md')) yield p
  }
}

for (const file of mdFiles(ROOT)) {
  const rel = relative(ROOT, file).replaceAll('\\', '/')
  readFileSync(file, 'utf8')
    .split(/\r?\n/)
    .forEach((line, i) => {
      if (line.includes('待建')) return
      for (const m of line.matchAll(/\]\(([^)#\s]+)(?:#[^)]*)?\)/g)) {
        const target = m[1]
        if (/^file:/i.test(target)) {
          problems.push(`${rel}:${i + 1}: 禁止 file:// 绝对路径（仅在本机有效）→ ${target}`)
          continue
        }
        if (/^(https?:|mailto:)/i.test(target)) continue
        if (!existsSync(resolve(dirname(file), decodeURI(target)))) {
          problems.push(`${rel}:${i + 1}: 链接目标不存在 → ${target}`)
        }
      }
    })
}

if (problems.length) {
  console.error(`✗ 全仓库链接检查未通过（${problems.length} 处）：`)
  for (const p of problems) console.error('  - ' + p)
  process.exit(1)
}
console.log('✓ 全仓库 Markdown 相对链接全部可达')
