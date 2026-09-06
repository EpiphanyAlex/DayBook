import { mockIPC, mockWindows, clearMocks } from '@tauri-apps/api/mocks'

export function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

export type IpcHandler = (args: Record<string, unknown>) => unknown
export function installMockIpc(handlers: Record<string, IpcHandler>) {
  mockWindows('main')
  const calls: { command: string; args: Record<string, unknown> }[] = []
  mockIPC((command, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>
    calls.push({ command, args })
    // 拖放只注册本地监听；未知命令直接失败，绝不回退真实 IPC。
    if (command === 'plugin:event|listen') return 1
    if (command === 'plugin:event|unlisten') return null
    const handler = handlers[command]
    if (!handler) throw new Error(`未配置的 mock IPC: ${command}`)
    return handler(args)
  }, { shouldMockEvents: true })
  return { calls, dispose: clearMocks }
}
