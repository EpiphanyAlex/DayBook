import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it } from 'vitest'
import { clearMocks } from '@tauri-apps/api/mocks'
import { App } from '../App'
import { fixtureHandlers, draft, total } from '../test/fixtures/limitedM1'
import { installMockIpc } from '../test/mockIpc'
import { mountReview, waitReady } from '../test/reviewScreen'
import { refreshReview } from './queries'

afterEach(() => { cleanup(); clearMocks() })
it('取消一条后编辑另一条，刷新保留排除意图', async () => {
  const { handlers } = fixtureHandlers()
  const ipc = installMockIpc(handlers)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><App /></QueryClientProvider>)
  await screen.findByDisplayValue('A-d2')
  fireEvent.click(screen.getAllByRole('button', { name: '取消选择' })[0])
  const merchant = screen.getByDisplayValue('A-d2')
  fireEvent.change(merchant, { target: { value: '修改商户' } })
  fireEvent.blur(merchant)
  await waitFor(() => expect(ipc.calls.some(c => c.command === 'update_draft')).toBe(true))
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 20)) })
  expect(screen.getAllByRole('button', { name: '取消选择' })).toHaveLength(1)
})

it('全部取消、切走再切回仍保留；新增默认选中；新 attempt 无旧排除意图', async () => {
  const { state, client } = mountReview()
  await waitReady()
  fireEvent.click(screen.getByRole('button', { name: '取消全选' }))
  const sources = screen.getAllByRole('button', { name: /SAY 一段口述/ })
  fireEvent.click(sources[1]); await screen.findByDisplayValue('B-d1')
  fireEvent.click(sources[0]); await screen.findByDisplayValue('A-d1')
  expect(screen.queryAllByRole('button', { name: '取消选择' })).toHaveLength(0)
  state.drafts.push(draft('added'))
  await act(async () => { await refreshReview(client, 'A') })
  await screen.findByDisplayValue('added')
  expect(screen.getAllByRole('button', { name: '取消选择' })).toHaveLength(1)
  state.sources[0].latestAttemptId = 'A-2'
  state.drafts = [draft('A-new', 'A', 'A-2')]
  state.totals = [total('A', 'A-2')]
  await act(async () => { await refreshReview(client, 'A') })
  await waitReady()
  expect(screen.getAllByRole('button', { name: '取消选择' })).toHaveLength(1)
})

it('确认参数只能包含当前来源和 attempt 的所选草稿', async () => {
  const { ipc } = mountReview()
  await waitReady()
  fireEvent.click(screen.getAllByRole('button', { name: '取消选择' })[0])
  fireEvent.click(screen.getByRole('button', { name: '确认所选入账' }))
  await waitFor(() => expect(ipc.calls.filter(c => c.command === 'confirm_drafts')).toHaveLength(1))
  expect(ipc.calls.find(c => c.command === 'confirm_drafts')?.args).toEqual({ draftIds: ['A-d2'], attestation: { fullSourceVisible: true, resultsAdjacent: true, itemCountVisible: true } })
})
