#!/usr/bin/env node
// README 中英同步检查。
// 用法（仓库任意位置）：node scripts/check-readme-sync.mjs
//
// 规则依据：仓库根 CLAUDE.md「文档层级」——改了 README.md 必须在同一个提交里同步
// README.en.md。中文是事实源，英文是镜像，不同步即缺陷。
//
// 判据（祖先关系，不是时间）：从 HEAD 出发、**不在 README.en.md 最后一次改动的历史里**
// 的、且改动了 README.md 的提交，数量必须为 0。
//   - 两份在同一个提交里改 ⇒ 该提交在镜像历史内 ⇒ 0 ⇒ 绿（这是正常路径）
//   - 只改中文 ⇒ 该提交不在镜像历史内 ⇒ ≥1 ⇒ 红，并列出是哪几个提交
//   - 只改英文（补译、修英文错字）⇒ 中文侧无新提交 ⇒ 0 ⇒ 绿
//
// 为什么不比提交时间：提交时间只到秒，同一秒内的两个提交分不出先后；`git rebase`
// 还会把一批提交的 committer date 统一重写成当前时刻。两者都会产生**假绿**——
// 这正是本脚本第一版在用例「只改中文」上漏判的原因。祖先关系不依赖时钟。
//
// 为什么不用 `git diff <base>...HEAD`：那种写法要一个对比基准，而 push 到 main 时
// 没有干净的基准，squash merge 后也对不上。本判据不依赖基准，四种情形（PR、push
// 到 main、squash merge、本地直接跑）结果一致；它判断的是「当前 HEAD 上英文镜像
// 是否落后」这一**持续状态**，而不是「这一个 PR 有没有带上」这一**一次性事件**。
//
// CI-ready：靠退出码报成败，非零即失败。
import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SOURCE = 'README.md'
const MIRROR = 'README.en.md'

const git = (...args) => execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' }).trim()

// 浅克隆下 git log / rev-list 可能看不到目标提交，从而给出一个**看起来正常的错误
// 结论**。宁可明确报错，也不静默误判。
if (git('rev-parse', '--is-shallow-repository') === 'true') {
  console.error('✗ 仓库是浅克隆，无法可靠判断 README 同步状态。')
  console.error('  CI：在 actions/checkout 上设 fetch-depth: 0；本地：git fetch --unshallow')
  process.exit(1)
}

for (const f of [SOURCE, MIRROR]) {
  if (!existsSync(join(ROOT, f))) {
    console.error(`✗ ${f} 不存在——中英两份 README 都是必需的（CLAUDE.md「文档层级」）。`)
    process.exit(1)
  }
}

const lastCommit = (f) => git('log', '-1', '--format=%H', '--', f)
const mirrorCommit = lastCommit(MIRROR)

// 文件在工作区存在但尚未进过任何提交：这是「还没提交」的中间状态，不是同步缺陷。
// CI 上不会出现（检出的都是已提交内容），只影响本地新建文件后立刻跑的情形。
if (!mirrorCommit || !lastCommit(SOURCE)) {
  console.log(`✓ README 同步检查跳过：${mirrorCommit ? SOURCE : MIRROR} 尚无提交记录`)
  process.exit(0)
}

// `^<mirrorCommit>` 排除镜像最后一次改动**及其全部祖先**，剩下的就是镜像没见过的
// 中文侧改动。分叉合并（中文在一支改、英文在另一支改）也会落到这里——镜像确实没
// 见过那些改动，报红是对的。
const unmirrored = git('log', '--oneline', 'HEAD', `^${mirrorCommit}`, '--', SOURCE)

if (unmirrored) {
  const commits = unmirrored.split('\n')
  console.error(`✗ README 中英不同步：${commits.length} 个提交改了 ${SOURCE} 而未同步 ${MIRROR}。`)
  for (const c of commits) console.error(`  - ${c}`)
  console.error(`  ${MIRROR} 最后改动：${git('log', '-1', '--format=%h %ad %s', '--date=short', '--', MIRROR)}`)
  console.error('  改了 README.md 必须在同一个提交里同步 README.en.md（CLAUDE.md「文档层级」）。')
  console.error('  纯中文措辞润色也请把对应措辞带到英文版——腐烂的镜像比没有镜像更糟。')
  process.exit(1)
}

console.log('✓ README 中英同步：英文镜像不落后于中文源')
