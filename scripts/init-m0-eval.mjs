#!/usr/bin/env node
// M0 正式样本清单初始化器的 Node 薄壳。真正的路径门禁与骨架生成在
// `daybook-eval init-m0`；本文件不读原件、不复制输入、不加载 backend。

import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const rust = join(root, 'src-tauri')
const HELP = `用法：
  node scripts/init-m0-eval.mjs --out <fixtures/local/新目录>
    [--screenshots 20..25] [--controls 3..5]
    [--single 3..4] [--two-to-three 8..10] [--four-plus 6..8]

默认：22 张截图、4 张对照、口述 4 + 9 + 7 = 20 段。
零额度、零 backend；只建中性 manifest 骨架与空 case 目录，不复制、不记录真实 input 路径。
输出不在 fixtures/local/（尤其 fixtures/ci/）会被拒绝。`

function argument(name) {
  const index = process.argv.indexOf(`--${name}`)
  return index === -1 ? undefined : process.argv[index + 1]
}

if (process.argv.includes('--help') || process.argv.includes('-h')) {
  console.log(HELP)
  process.exit(0)
}
const valueFlags = new Set([
  '--out',
  '--screenshots',
  '--controls',
  '--single',
  '--two-to-three',
  '--four-plus',
])
for (let index = 2; index < process.argv.length; index += 2) {
  const flag = process.argv[index]
  const value = process.argv[index + 1]
  if (!valueFlags.has(flag)) {
    console.error(`[eval] 不认识的参数 ${flag}\n\n${HELP}`)
    process.exit(1)
  }
  if (!value || value.startsWith('--')) {
    console.error(`[eval] ${flag} 缺参数值\n\n${HELP}`)
    process.exit(1)
  }
}
const out = argument('out')
if (!out || out.startsWith('--')) {
  console.error(`[eval] 缺 --out\n\n${HELP}`)
  process.exit(1)
}

const binary = join(rust, 'target', 'debug', 'daybook-eval')
const args = ['init-m0', '--root', root, '--out', out]
for (const name of ['screenshots', 'controls', 'single', 'two-to-three', 'four-plus']) {
  const value = argument(name)
  if (value !== undefined) args.push(`--${name}`, value)
}
const command = existsSync(binary)
  ? { executable: binary, args, cwd: root }
  : {
      executable: 'cargo',
      args: ['run', '--offline', '--quiet', '--bin', 'daybook-eval', '--', ...args],
      cwd: rust,
    }
const result = spawnSync(command.executable, command.args, {
  cwd: command.cwd,
  encoding: 'utf8',
  stdio: 'inherit',
})
if (result.error) {
  console.error(`[eval] 无法启动 daybook-eval：${result.error.message}`)
  process.exit(1)
}
process.exit(result.status ?? 1)
