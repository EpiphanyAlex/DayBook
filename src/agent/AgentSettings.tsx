import { useEffect, useState } from 'react'
import type { AgentSelection, BackendStatus } from '../review/types'

interface Props {
  selection: AgentSelection | null
  status: BackendStatus | null
  disabled: boolean
  onSave: (selection: AgentSelection) => Promise<void>
  onProbe: () => Promise<void>
}

export function AgentSettings({ selection, status, disabled, onSave, onProbe }: Props) {
  const [backendId, setBackendId] = useState<AgentSelection['backendId']>('claude-code')
  const [mode, setMode] = useState<'auto' | 'specific'>('auto')
  const [modelId, setModelId] = useState('')
  useEffect(() => {
    if (!selection) return
    setBackendId(selection.backendId)
    setMode(selection.modelSelection.mode)
    setModelId(selection.modelSelection.mode === 'specific' ? selection.modelSelection.modelId : '')
  }, [selection])

  const visibleStatus = status?.backendId === backendId ? status : null
  const candidate = visibleStatus?.models ?? null
  const validId = /^[A-Za-z0-9._-]{1,128}$/.test(modelId) && modelId !== 'default' && !(backendId === 'claude-code' && ['opus', 'sonnet', 'haiku', 'fable'].includes(modelId))
  const next: AgentSelection = { backendId, modelSelection: mode === 'auto' ? { mode: 'auto' } : { mode: 'specific', modelId } }
  const changed = JSON.stringify(next) !== JSON.stringify(selection)

  return <section className="agent-settings" aria-label="解析引擎与模型">
    <label htmlFor="agent-backend">解析引擎</label>
    <select id="agent-backend" value={backendId} onChange={event => setBackendId(event.target.value as AgentSelection['backendId'])} disabled={disabled}>
      <option value="claude-code">Claude Code</option>
      <option value="codex">Codex</option>
    </select>
    <label htmlFor="agent-model-mode">模型</label>
    <select id="agent-model-mode" value={mode} onChange={event => setMode(event.target.value as 'auto' | 'specific')} disabled={disabled}>
      <option value="auto">自动（由所选 CLI 决定）</option>
      <option value="specific">指定固定模型 ID</option>
    </select>
    {mode === 'specific' && <>
      <label htmlFor="agent-model-id">固定模型 ID</label>
      <input id="agent-model-id" className="field-boxed" value={modelId} onChange={event => setModelId(event.target.value)} list="agent-model-candidates" disabled={disabled} placeholder="输入完整模型 ID" />
      <datalist id="agent-model-candidates">{candidate?.map(item => <option key={item.modelId} value={item.modelId}>{item.displayName}</option>)}</datalist>
      <small>候选清单不代表账号有权使用。实际模型以本次解析结果为准。</small>
    </>}
    <small>账号：{visibleStatus?.authenticated === true ? '已验证' : visibleStatus?.authenticated === false ? '未登录' : '无法确认'} · 额度：{visibleStatus?.quota === 'available' ? '可用' : visibleStatus?.quota === 'exhausted' ? '已用完' : '暂不可获取'} · 模型清单：{candidate ? `${candidate.length} 个候选` : '未知'}</small>
    <div>
      <button type="button" disabled={disabled || !changed || (mode === 'specific' && !validId)} onClick={() => void onSave(next)}>保存选择</button>
      <button type="button" disabled={disabled || changed} onClick={() => void onProbe()}>检查解析器</button>
    </div>
  </section>
}
