#!/usr/bin/env node
//
// 夹具导出器的薄壳（docs/prd/07-eval.md §3.6）。
//
// 把一次真实解析会话打包成一个自包含、可重放的夹具目录。**打包动作在 Rust 里**
// （`src-tauri/src/eval/export.rs`），这里只负责起进程与算默认输出路径。
//
// 用法：
//   node scripts/export-fixture.mjs <agent_session_id> [--slug <name>] [--data-dir <path>] [--out <path>]
//
// `<agent_session_id>` 取自 `<数据目录>/logs/` 下的文件名，或 `parse_attempts.agent_session_id`。
//
// **导出的一律是真实截图与真实金额**，所以默认写 `fixtures/local/<date>-<slug>/`——
// 那一支不进 git（§3.7）。要进 CI 集必须先脱敏或改用合成样本再手工移过去；
// 导出器自己会拒绝直接写进 `fixtures/ci/`。
//
// **原料只在 `debug` 级日志里**：`trace` 级只记工具参数的形状，重放不出来（ADR-0007）。
// 那次解析跑的时候 debug 开关关着，或日志已过保留期，导出会明说是这两个原因之一。

import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const rust = join(root, 'src-tauri')

const argv = process.argv.slice(2)
const session = argv.find((argument) => !argument.startsWith('--'))
if (!session) {
  console.error(
    [
      '[export-fixture] 缺 <agent_session_id>。',
      '',
      '  用法：node scripts/export-fixture.mjs <agent_session_id> [--slug <name>] [--data-dir <path>]',
      '',
      '  会话 id 取自 <数据目录>/logs/ 下的文件名，或 parse_attempts.agent_session_id。',
    ].join('\n'),
  )
  process.exit(1)
}

function flag(name) {
  const index = argv.indexOf(`--${name}`)
  return index === -1 ? undefined : argv[index + 1]
}

const passthrough = ['--session', session, '--root', root]
for (const name of ['slug', 'data-dir', 'out']) {
  const value = flag(name)
  if (value !== undefined) passthrough.push(`--${name}`, value)
}

// 优先用已构建好的二进制；单独跑本脚本时退回 `cargo run`。
const binary = join(rust, 'target', 'debug', 'daybook-eval')
const useBinary = existsSync(binary)
const result = spawnSync(
  useBinary ? binary : 'cargo',
  useBinary
    ? ['export-fixture', ...passthrough]
    : ['run', '--offline', '--quiet', '--bin', 'daybook-eval', '--', 'export-fixture', ...passthrough],
  { cwd: useBinary ? root : rust, stdio: 'inherit' },
)
if (result.error) {
  console.error(`[export-fixture] 无法启动 daybook-eval：${result.error.message}`)
  process.exit(1)
}
process.exit(result.status ?? 1)
