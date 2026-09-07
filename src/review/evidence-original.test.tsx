import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { clearMocks } from '@tauri-apps/api/mocks'
import { EvidencePane } from './EvidencePane'
import { verifiedSpan } from './evidenceSpan'
import { draft, evidence } from '../test/fixtures/limitedM1'
import { fixtureHandlers } from '../test/fixtures/limitedM1'
import { mountReview, waitReady } from '../test/reviewScreen'

afterEach(() => { cleanup(); clearMocks() })

it('完整口述保留 emoji，只按可验证 code-point span 定位', () => {
  const d = { ...draft(), evidenceText: '🧾咖啡', evidenceSpanStart: 1, evidenceSpanEnd: 4 }
  const text = '中🧾咖啡，中🧾咖啡'
  expect(verifiedSpan(text, d)).toEqual(['中', '🧾咖啡', '，中🧾咖啡'])
  for (const pair of [[null, null], [-1, 4], [1, 99], [1, 1], [0, 3]]) {
    expect(verifiedSpan(text, { ...d, evidenceSpanStart: pair[0], evidenceSpanEnd: pair[1] })).toBeNull()
  }
  render(<EvidencePane sourceId="A" content={{ ...evidence(), text }} draft={d} error={null} onRetry={() => {}} onImageState={() => {}} />)
  expect(screen.getByTestId('original-text').textContent).toBe(text)
  expect(document.querySelectorAll('mark')).toHaveLength(1)
})

it('整屏读取失败拒绝背书，显式重读成功后恢复且不调用 agent', async () => {
  let fail = true
  const { ipc } = mountReview({ read_evidence: () => { if (fail) throw new Error('合成原件读取失败'); return evidence() } })
  await screen.findByText(/原件读取失败：合成原件读取失败/)
  expect(screen.getByRole('button', { name: '确认所选入账' })).toBeDisabled()
  expect(screen.queryByTestId('original-text')).not.toBeInTheDocument()
  fail = false; fireEvent.click(screen.getByRole('button', { name: '重读原件' }))
  await waitReady()
  expect(ipc.calls.filter(c => c.command === 'read_evidence')).toHaveLength(2)
  expect(ipc.calls.filter(c => c.command === 'parse_source')).toHaveLength(0)
})

it('任一口述声明 span 无法验证时批量背书暂停，单条核对仍可用', async () => {
  const fixture = fixtureHandlers()
  fixture.state.drafts[1].evidenceSpanEnd = 999
  mountReview({}, fixture)
  await screen.findByDisplayValue('A-d2')
  expect(screen.getByRole('button', { name: '确认所选入账' })).toBeDisabled()
  expect(screen.getAllByRole('button', { name: '单条确认' })[0]).toBeEnabled()
})

it('原生焦点改变当前声明而不改变勾选；编辑只重读投影，不重读不可变原件', async () => {
  const fixture = fixtureHandlers()
  fixture.state.drafts[1] = { ...fixture.state.drafts[1], sourceOrdinal: 2, evidenceText: '午饭24元', evidenceSpanStart: 9, evidenceSpanEnd: 14 }
  const { ipc } = mountReview({}, fixture)
  await waitReady()
  fireEvent.focus(screen.getByRole('article', { name: '草稿 2 A-d2' }))
  expect(within(screen.getByText('当前草稿 · 抽取声明').parentElement!).getByText('午饭24元')).toBeInTheDocument()
  expect(screen.getAllByRole('button', { name: '取消选择' })).toHaveLength(2)
  const amount = screen.getAllByLabelText('金额')[1]
  fireEvent.change(amount, { target: { value: '24.00' } }); fireEvent.blur(amount)
  await waitFor(() => expect(ipc.calls.some(c => c.command === 'update_draft')).toBe(true))
  await waitReady()
  expect(ipc.calls.filter(c => c.command === 'read_evidence')).toHaveLength(1)
  expect(ipc.calls.find(c => c.command === 'update_draft')?.args.patch).toEqual({ amountMinor: '2400' })
})

it('无效 span 保留全文与声明，不搜索替代位置', () => {
  render(<EvidencePane sourceId="A" content={evidence()} draft={{ ...draft(), evidenceSpanEnd: 99 }} error={null} onRetry={() => {}} onImageState={() => {}} />)
  expect(screen.getByTestId('original-text').textContent).toBe(evidence().text)
  expect(screen.getByText(/无法定位/)).toBeInTheDocument()
  expect(document.querySelector('mark')).toBeNull()
})

it('原件读取与图片解码失败各有重试，不继续显示旧内容', () => {
  const retry = vi.fn(), imageState = vi.fn()
  const props = { sourceId: 'A', draft: draft(), onRetry: retry, onImageState: imageState }
  const view = render(<EvidencePane {...props} content={null} error={new Error('读取失败')} />)
  fireEvent.click(screen.getByRole('button', { name: '重读原件' }))
  expect(retry).toHaveBeenCalledOnce()
  view.rerender(<EvidencePane {...props} content={{ kind: 'file', mimeType: 'image/png', dataBase64: 'broken', text: null }} error={null} />)
  fireEvent.error(screen.getByRole('img'))
  expect(screen.getByText(/图片无法解码/)).toBeInTheDocument()
  expect(imageState).toHaveBeenLastCalledWith(false)
})
