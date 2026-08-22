import { describe, expect, it } from 'vitest'
import { backendPresentation, type BackendState } from './presentation'

function state(overrides: Partial<BackendState>): BackendState {
  return {
    available: true,
    availabilityReason: null,
    authenticated: null,
    ready: false,
    errorCode: null,
    ...overrides,
  }
}

describe('agent/backend-guidance', () => {
  it('gives three different repairs for the three installation failures', () => {
    const missing = backendPresentation(
      state({
        available: false,
        availabilityReason: 'not_found',
        errorCode: 'agent.backend_unavailable',
      }),
    )
    const notExecutable = backendPresentation(
      state({
        available: false,
        availabilityReason: 'not_executable',
        errorCode: 'agent.backend_unavailable',
      }),
    )
    const versionUnreadable = backendPresentation(
      state({
        available: false,
        availabilityReason: 'version_unreadable',
        errorCode: 'agent.backend_unavailable',
      }),
    )

    expect(missing.label).toContain('未安装')
    expect(missing.instruction).toContain('安装')
    expect(notExecutable.instruction).toContain('chmod')
    expect(versionUnreadable.instruction).toContain('--version')
    // 三种原因给同一句指引等于没指引（01 §3.5）。
    const instructions = new Set(
      [missing, notExecutable, versionUnreadable].map((view) => view.instruction),
    )
    expect(instructions.size).toBe(3)
    expect(notExecutable.label).not.toContain('未安装')
    expect(versionUnreadable.label).not.toContain('未安装')
  })

  it('distinguishes installation from login', () => {
    const missing = backendPresentation(
      state({
        available: false,
        availabilityReason: 'not_found',
        errorCode: 'agent.backend_unavailable',
      }),
    )
    const loggedOut = backendPresentation(
      state({ authenticated: false, errorCode: 'agent.not_authenticated' }),
    )

    expect(loggedOut.label).toContain('尚未登录')
    expect(loggedOut.instruction).toContain('终端运行 claude')
    expect(missing.instruction).not.toBe(loggedOut.instruction)
  })

  it('says it is still checking until the probe has succeeded', () => {
    // 修正前这一档兜底成 `ready: true`：probe 还没跑完，界面已经说「考古员已就绪」。
    const checking = backendPresentation(state({ available: true }))
    expect(checking.ready).toBe(false)
    expect(checking.label).toContain('正在检查')
    expect(checking.label).not.toContain('已就绪')
    expect(backendPresentation(null).label).toContain('正在检查')
  })

  it('explains why an unsealed surface is blocked', () => {
    const presentation = backendPresentation(
      state({ authenticated: true, errorCode: 'agent.tool_surface_unsealed' }),
    )
    expect(presentation.ready).toBe(false)
    expect(presentation.instruction).toContain('额外工具')
  })

  it('keeps other probe failures visible without calling them uninstalled', () => {
    const presentation = backendPresentation(state({ errorCode: 'agent.spawn_failed' }))
    expect(presentation.ready).toBe(false)
    expect(presentation.label).not.toContain('未安装')
    expect(presentation.instruction).toContain('agent.spawn_failed')
  })

  it('is ready only when the runtime says so', () => {
    const ready = backendPresentation(state({ authenticated: true, ready: true }))
    expect(ready.ready).toBe(true)
    expect(ready.label).toContain('已就绪')
  })
})
