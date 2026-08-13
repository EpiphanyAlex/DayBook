#!/usr/bin/env node
// 规格不变式检查：抓**决策漂移**，不抓格式问题。
// 用法（仓库任意位置）：node scripts/check-spec-invariants.mjs
//
// 为什么需要第五条门禁（2026-08-10 建立）：
//   另外四条门禁——check-docs.mjs（frontmatter + 链接）、check-links.mjs（全仓链接）、
//   check-readme-sync.mjs（中英同步）——在一轮把「口述合计恒为空」改成
//   「通常为空、说了就对账」的改动之后，**全部是绿的**，而七处文档仍在说旧结论。
//   它们查的是格式、可达性与提交关系，**查不了「这句话和上周拍的决定相反」**。
//
//   本脚本不试图理解语义，只做一件很笨但有效的事：
//   **把几条最容易复发的旧措辞列成禁用表，在现行章节里出现就红。**
//   规则少而准，比多而糊有用——每加一条都要能说出「它防的是哪一次真实回退」。
//
// 豁免（两种，都必须是显式的）：
//   1. 「变更记录」/「回流记录」章节整段跳过——它们的职责就是引用旧结论
//   2. 行尾 <!-- legacy --> 标记：现行正文里确实需要引用旧措辞时（解释「原来错在哪」），
//      作者必须显式标出来。**不提供隐式启发式**（例如「含 v0.x 就算历史」）——
//      那会让检查器在最该报警的地方悄悄闭嘴。
//
// CI-ready：靠退出码报成败，非零即失败。
import { readdirSync, readFileSync } from 'node:fs'
import { dirname, join, resolve, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'dist', '.vite'])
const SELF = 'scripts/check-spec-invariants.mjs'

// 每条规则都要写清「防的是哪一次回退」——说不出来的规则不该加。
const RULES = [
  {
    // 防的是 2026-08-14 定位回写时发现的三处真实漂移：AGENTS.md、品牌说明与
    // 应用空状态仍把「回溯记录器」当作品类，尽管总 PRD 已改为「个人事务助理」。
    id: 'retroactive-recorder-positioning',
    re: /\*\*回溯记录器\*\*|你的回溯记录器|retroactive recorder/i,
    why: '「个人事务助理」是品类定位；「回溯优先」只是设计原则（总 PRD §1）',
    fix: '品类改用「个人事务助理」；表达工作方式时改用「回溯优先」',
  },
  {
    id: 'declared-total',
    re: /declared_total_/,
    why: '声明合计已改名 `reported_total_*` 并移到 `parse_attempts`（00 §3.6「声明合计归尝试，不归来源」）',
    fix: '改用 `reported_total_*`；确需引用旧名请在行尾加 <!-- legacy -->',
  },
  {
    id: 'utterance-total-always-empty',
    re: /(utterance|口述)[^\n]{0,40}(合计|reported_total_\*)[^\n]{0,20}恒为空|恒为空[^\n]{0,20}(utterance|口述)/,
    why: '口述**通常**没有合计，但用户说「总共 100」时照常对账（00 §3.6、03 §3.3）',
    fix: '改成「通常为空；用户明说合计时照常对账，确认策略仍是 user_attested_batch」',
  },
  {
    id: 'utterance-always-na',
    re: /(utterance|口述|语音)[^\n]{0,40}恒为\s*\**`?not_applicable/,
    why: '同上——`not_applicable` 只在「本次没报合计」时成立，不是口述来源的恒定属性',
    fix: '把「恒为」改成「通常为 / 没报合计时为」',
  },
  {
    id: 'na-grants-batch',
    re: /(not_applicable|NotApplicable)[^\n]{0,24}(放行|允许)[^\n]{0,8}批量/,
    why: '放行批量确认的是 `confirmation_policy`，不是 `reconciliation_status`（03 §3.3「两个维度」）',
    fix: '改成「`user_attested_batch` 放行批量」——把两个维度焊回一起正是这条要防的',
  },
  {
    // 防的是 2026-08-13 实现验收发现的那一起：根 CLAUDE.md 约束 5 无条件写着
    // 「不符时阻止批量入库」，而 03 §3.3 自 2026-08-10 起规定 utterance 的确认策略
    // 与对账结果无关。四条门禁全绿，顶层文件与三份下游文档一直在说相反的话。
    // 行内出现 kind 限定词（utterance / 口述 / user_attested / kind = file）即豁免。
    id: 'mismatch-blocks-all-batch',
    re: /^(?![^\n]*(utterance|口述|user_attested|kind\s*=\s*`?file))[^\n]*(不符|对不上|failed)[^\n]{0,24}(阻止|禁止)[^\n]{0,6}批量/,
    why: '对账 `failed` 只对 `kind = file` 阻止批量；`utterance` 的确认策略与对账结果无关（03 §3.3）',
    fix: '补上 `kind` 限定，或点名 `user_attested_batch` 这道人工闸门',
  },
  {
    id: 'probe-equals-tools-list',
    re: /(有效工具集|能力清单|capability manifest)[^\n]{0,30}(就是|等同于|即)[^\n]{0,20}tools\/list/,
    why: 'MCP `tools/list` 看不见 `Bash` / `Read` / `Edit`，也看不见 hook（01 §3.7）',
    fix: '探测必须覆盖全部能力来源，并对整份 capability manifest 计算指纹',
  },
  {
    id: 'total-check-by-source',
    re: /total_check\(\s*source_id\s*\)/,
    why: '总额校验的入参是 `attempt_id`——按来源求和会混进重试的另一次输出（03 §3.3）',
    fix: '改成 `total_check(attempt_id)`',
  },
  {
    id: 'm0-table-count',
    re: /M0[^\n]{0,12}(四|五)张表|(四|五)张\s*M0\s*表/,
    why: 'M0 已是六张表（含 `parse_attempts` 与 `accounts` 骨架）',
    fix: '改成六张；这个数漂移过两次，所以列进不变式',
  },
  {
    id: 'unknown-currency-fallback',
    re: /未知币种(?![^\n]*(?:不是|不应|不得|不回退|缺陷|错误|拒绝|❌))[^\n]{0,40}(回退|回落)[^\n]{0,12}(exponent\s*=\s*)?2|表里没有的币种(?![^\n]*(?:非法|不是|不应|不得|不回退|缺陷|错误|拒绝|❌))[^\n]{0,40}(回退|按)[^\n]{0,12}2/,
    why: '未知币种已改为 `data.unsupported_currency` 并拒绝写入（00 §3.4）',
    fix: '改成拒绝写入；解释旧结论的历史行请放进回流/变更记录或标记 legacy',
  },
  {
    id: 'frontend-total-check-by-source',
    re: /total_check['"`]?\s*,\s*\{\s*sourceId|confirm_batch['"`]?\s*,\s*\{\s*sourceId/,
    why: '总额校验与批量确认按 `attempt_id` 定位，按来源会把重试输出混在一起',
    fix: '前端参数改为 `attemptId`',
  },
  {
    id: 'failed-source-zero-history',
    re: /failed_source_has_no_drafts|失败后[^\n]{0,30}关联草稿数为\s*0/,
    why: '失败尝试的草稿置 `voided_at` 并保留历史，只有「未作废草稿数为 0」成立',
    fix: '改为 `failed_source_has_no_active_drafts` 并按 `voided_at IS NULL` 断言',
  },
]

const HEADING = /^(#{1,6})\s*(.+?)\s*$/
// 标题常带序号（「## 7. 回流记录」），所以不锚定行首
const SKIP_SECTION = /(变更记录|回流记录|Changelog)/i

function* mdFiles(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (SKIP_DIRS.has(e.name)) continue
    const p = join(dir, e.name)
    if (e.isDirectory()) yield* mdFiles(p)
    else if (e.name.endsWith('.md')) yield p
  }
}

const problems = []

for (const file of mdFiles(ROOT)) {
  const rel = relative(ROOT, file).replaceAll('\\', '/')
  // 跳过尚未创建目录里的模板等：这里只扫真实存在的 .md，无需额外过滤。
  let skipDepth = 0 // 非 0 表示正处在被跳过的章节里，值为该章节标题的层级
  readFileSync(file, 'utf8')
    .split(/\r?\n/)
    .forEach((line, i) => {
      const h = line.match(HEADING)
      if (h) {
        const level = h[1].length
        if (skipDepth && level <= skipDepth) skipDepth = 0 // 同级或更高级标题结束跳过
        if (!skipDepth && SKIP_SECTION.test(h[2])) skipDepth = level
      }
      if (skipDepth) return
      if (line.includes('<!-- legacy -->')) return

      for (const r of RULES) {
        if (r.re.test(line)) {
          problems.push({ rel, line: i + 1, rule: r, text: line.trim().slice(0, 120) })
        }
      }
    })
}

// 本脚本自身写着全部禁用词，必然自命中——排除掉。
const real = problems.filter((p) => p.rel !== SELF)

if (real.length) {
  console.error('✗ 规格不变式检查未通过：\n')
  for (const p of real) {
    console.error(`  ${p.rel}:${p.line}  [${p.rule.id}]`)
    console.error(`    ${p.text}`)
    console.error(`    为什么：${p.rule.why}`)
    console.error(`    怎么改：${p.rule.fix}\n`)
  }
  console.error(
    `共 ${real.length} 处。确需在现行正文里引用旧措辞（例如解释「原来错在哪」），` +
      '在该行末尾加 <!-- legacy --> 显式标注。',
  )
  process.exit(1)
}

console.log('✓ 规格不变式检查通过：现行章节无已知的旧结论残留')
