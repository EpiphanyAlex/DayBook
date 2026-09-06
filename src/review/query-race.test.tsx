import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it } from 'vitest'
import { clearMocks } from '@tauri-apps/api/mocks'
import { App } from '../App'
import { fixtureHandlers, draft, total, evidence } from '../test/fixtures/limitedM1'
import { deferred, installMockIpc } from '../test/mockIpc'
import { serializeIpcIntegers } from '../lib/bridge'
import { mountReview, waitReady } from '../test/reviewScreen'
import { refreshSources, refreshReview } from './queries'

afterEach(() => { cleanup(); clearMocks() })
it('A→B：A 的迟到草稿不能覆盖 B', async () => {
  const { handlers } = fixtureHandlers()
  const late = deferred<unknown>()
  installMockIpc({ ...handlers, list_active_drafts: args => args.sourceId === 'A' ? late.promise : handlers.list_active_drafts(args) })
  render(<QueryClientProvider client={new QueryClient()}><App /></QueryClientProvider>)
  const sources = await screen.findAllByRole('button', { name: /SAY 一段口述/ })
  fireEvent.click(sources[1])
  await screen.findByDisplayValue('B-d1')
  await act(async () => late.resolve(serializeIpcIntegers([draft()])))
  expect(screen.getByDisplayValue('B-d1')).toBeInTheDocument()
  expect(screen.queryByDisplayValue('A-d1')).not.toBeInTheDocument()
})

it('同来源 attempt 1→2：旧草稿、空数组与合计均不能放行新尝试', async () => {
  const fixture = fixtureHandlers()
  const lateDrafts = deferred<unknown>(), lateTotal = deferred<unknown>()
  let draftReads = 0
  const { client } = mountReview({
    list_active_drafts: args => args.sourceId === 'A' && ++draftReads === 1 ? lateDrafts.promise : fixture.handlers.list_active_drafts(args),
    check_source_total: args => args.attemptId === 'A-1' ? lateTotal.promise : fixture.handlers.check_source_total(args),
  }, fixture)
  await waitFor(() => expect(draftReads).toBe(1))
  fixture.state.sources[0].latestAttemptId = 'A-2'
  fixture.state.drafts = [draft('new', 'A', 'A-2')]
  fixture.state.totals = [total('A', 'A-2')]
  await act(async () => { await refreshSources(client) })
  await screen.findByDisplayValue('new')
  await waitReady()
  await act(async () => { lateDrafts.resolve([]); lateTotal.resolve(serializeIpcIntegers(total())) })
  expect(screen.getByDisplayValue('new')).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: '确认所选入账' }))
})

it('DTO attempt 错配使审核读取失败并暂停确认，显式重读可恢复', async () => {
  const fixture = fixtureHandlers()
  let wrong = true
  mountReview({ list_active_drafts: args => wrong ? serializeIpcIntegers([draft('old', 'A', 'wrong')]) : fixture.handlers.list_active_drafts(args) }, fixture)
  await screen.findByText(/草稿与当前解析尝试不一致/)
  expect(screen.getByRole('button', { name: '确认所选入账' })).toBeDisabled()
  wrong = false
  fireEvent.click(screen.getByRole('button', { name: '重读审核数据' }))
  await waitReady()
})

it('原件与合计的迟到响应也只留在发起来源', async () => {
  const fixture = fixtureHandlers()
  const lateEvidence = deferred<unknown>(), lateTotal = deferred<unknown>()
  mountReview({ read_evidence: args => args.sourceId === 'A' ? lateEvidence.promise : { ...evidence(), text: 'B 的完整原文' },
    check_source_total: args => args.attemptId === 'A-1' ? lateTotal.promise : fixture.handlers.check_source_total(args) }, fixture)
  fireEvent.click((await screen.findAllByRole('button', { name: /SAY 一段口述/ }))[1])
  await screen.findByText('B 的完整原文')
  await act(async () => { lateEvidence.resolve(evidence()); lateTotal.resolve(serializeIpcIntegers(total())) })
  expect(screen.getByTestId('original-text')).toHaveTextContent('B 的完整原文')
  expect(screen.getByDisplayValue('B-d1')).toBeInTheDocument()
})

it('StrictMode、切来源、窗口聚焦与显式重读不会自动重发 probe/parse', async () => {
  const { client, ipc } = mountReview()
  await waitReady()
  await act(async () => { window.dispatchEvent(new Event('focus')); window.dispatchEvent(new Event('online')); await refreshReview(client, 'A') })
  expect(ipc.calls.filter(c => c.command === 'probe_agent')).toHaveLength(1)
  expect(ipc.calls.filter(c => c.command === 'parse_source')).toHaveLength(0)
})
