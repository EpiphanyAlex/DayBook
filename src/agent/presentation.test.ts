import { describe, expect, it } from 'vitest'
import { backendPresentation } from './presentation'

describe('agent/backend-guidance', () => {
  it('distinguishes installation from login', () => {
    const missing = backendPresentation({
      available: false,
      authenticated: null,
      errorCode: 'agent.backend_unavailable',
    })
    const loggedOut = backendPresentation({
      available: true,
      authenticated: false,
      errorCode: 'agent.not_authenticated',
    })

    expect(missing.label).toContain('未安装')
    expect(missing.instruction).toContain('安装')
    expect(loggedOut.label).toContain('尚未登录')
    expect(loggedOut.instruction).toContain('终端运行 claude')
    expect(missing.instruction).not.toBe(loggedOut.instruction)
  })

  it('explains why an unsealed surface is blocked', () => {
    const presentation = backendPresentation({
      available: true,
      authenticated: true,
      errorCode: 'agent.tool_surface_unsealed',
    })
    expect(presentation.ready).toBe(false)
    expect(presentation.instruction).toContain('额外工具')
  })
})
