import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { AgentSettings } from './AgentSettings'
import type { AgentSelection, BackendStatus } from '../review/types'

describe('agent/model-selection', () => {
  it('keeps backend, model, account and readiness separate', () => {
    const onSave = vi.fn(async () => {})
    const selection: AgentSelection = { backendId: 'claude-code', modelSelection: { mode: 'auto' } }
    const codex: BackendStatus = {
      backendId: 'codex', available: true, availabilityReason: null,
      authenticated: null, ready: false, errorCode: 'agent.tool_surface_unsealed',
      version: '0.154.0', quota: 'unknown',
      models: [{ modelId: 'gpt-test-fixed', displayName: 'Test', supportsImages: true }],
    }
    render(<AgentSettings selection={selection} status={codex} disabled={false} onSave={onSave} onProbe={async () => {}} />)
    fireEvent.change(screen.getByLabelText('解析引擎'), { target: { value: 'codex' } })
    fireEvent.change(screen.getByLabelText('模型'), { target: { value: 'specific' } })
    fireEvent.change(screen.getByLabelText('固定模型 ID'), { target: { value: 'gpt-test-fixed' } })
    expect(screen.getByText(/账号：无法确认/)).toBeInTheDocument()
    expect(screen.getByText(/额度：暂不可获取/)).toBeInTheDocument()
    expect(screen.getByText(/模型清单：1 个候选/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '保存选择' }))
    expect(onSave).toHaveBeenCalledWith({ backendId: 'codex', modelSelection: { mode: 'specific', modelId: 'gpt-test-fixed' } })
  })
})
