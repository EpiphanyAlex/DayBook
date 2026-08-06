#!/usr/bin/env node
// docs/prd 机械自检：frontmatter 必填字段 + 相对链接可达。
// 用法（仓库任意位置）：node docs/prd/check-docs.mjs
// 规则来源：docs/prd/CLAUDE.md（硬规则 3/4）。CI-ready：退出码非零即失败。
// 已知局限：不校验 #锚点（中文标题 slug 规则易误报，列为 TODO）；
//          不检测语义性悬空指称（那是人工自检的部分，见 CLAUDE.md 交付自检 2）。
// 注：本项目不用 ticket（依据仓库根 CLAUDE.md「PRD 体系与工作流」），
//    因此只有一套 frontmatter 必填字段，没有 ticket 类的第二套。
import { readdirSync, readFileSync, existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = dirname(fileURLToPath(import.meta.url)) // …/docs/prd
const problems = []

function* mdFiles(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name)
    if (e.isDirectory()) yield* mdFiles(p)
    else if (e.name.endsWith('.md')) yield p
  }
}

const REQUIRED_FIELDS = ['title', 'status', 'owner', 'date', 'version']

for (const file of mdFiles(ROOT)) {
  const rel = file.slice(ROOT.length + 1).replaceAll('\\', '/')
  if (rel === 'CLAUDE.md') continue // 规则文件自身豁免 frontmatter
  const text = readFileSync(file, 'utf8')

  // 1) frontmatter 必填
  const fm = text.match(/^---\r?\n([\s\S]*?)\r?\n---/)
  if (!fm) {
    problems.push(`${rel}: 缺 frontmatter（文件头 --- 块）`)
  } else {
    for (const f of REQUIRED_FIELDS) {
      if (!new RegExp(`^${f}\\s*:`, 'm').test(fm[1])) {
        problems.push(`${rel}: frontmatter 缺必填字段 \`${f}\``)
      }
    }
  }

  // 2) 相对链接可达（同一行含「待建」豁免；http/mailto/纯锚点跳过）
  text.split(/\r?\n/).forEach((line, i) => {
    if (line.includes('待建')) return
    for (const m of line.matchAll(/\]\(([^)#\s]+)(?:#[^)]*)?\)/g)) {
      const target = m[1]
      if (/^(https?:|mailto:)/.test(target)) continue
      const abs = resolve(dirname(file), decodeURI(target))
      if (!existsSync(abs)) {
        problems.push(`${rel}:${i + 1}: 链接目标不存在 → ${target}`)
      }
    }
  })
}

if (problems.length) {
  console.error(`✗ docs/prd 自检未通过（${problems.length} 处）：`)
  for (const p of problems) console.error('  - ' + p)
  process.exit(1)
}
console.log('✓ docs/prd 自检通过：frontmatter 完整，相对链接全部可达')
