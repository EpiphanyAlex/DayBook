import { act, cleanup, fireEvent, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it } from 'vitest'
import { clearMocks } from '@tauri-apps/api/mocks'
import { mountReview, waitReady } from '../test/reviewScreen'
import { deferred } from '../test/mockIpc'
import { fixtureHandlers, draft, total } from '../test/fixtures/limitedM1'
import { refreshReview } from './queries'

afterEach(() => { cleanup(); clearMocks() })
const failure = () => { throw new Error('合成写入失败') }

it('编辑失败保留输入及未保存提示，阻止确认；恢复不发写命令', async () => {
  const { ipc } = mountReview({ update_draft: failure })
  await waitReady()
  const merchant = screen.getByDisplayValue('A-d1')
  fireEvent.change(merchant, { target: { value: '尚未保存的商户' } }); fireEvent.blur(merchant)
  await screen.findByText(/未保存：合成写入失败/)
  expect(merchant).toHaveValue('尚未保存的商户')
  expect(screen.getByRole('button', { name: '确认所选入账' })).toBeDisabled()
  fireEvent.click(screen.getByRole('button', { name: '恢复已保存值' }))
  await waitReady()
  expect(merchant).toHaveValue('A-d1')
  expect(ipc.calls.filter(c => c.command === 'update_draft')).toHaveLength(1)
})

it('保存成功后的旧回调不覆盖后来输入，也不抢回来源焦点', async () => {
  const saved = deferred<unknown>()
  mountReview({ update_draft: () => saved.promise })
  await waitReady()
  const merchant = screen.getByDisplayValue('A-d1')
  fireEvent.change(merchant, { target: { value: '第一次编辑' } }); fireEvent.blur(merchant)
  fireEvent.change(merchant, { target: { value: '后来编辑' } })
  fireEvent.click(screen.getAllByRole('button', { name: /SAY 一段口述/ })[1])
  await screen.findByDisplayValue('B-d1')
  await act(async () => saved.resolve(null))
  await waitFor(() => expect(screen.getByDisplayValue('B-d1')).toBeInTheDocument())
  fireEvent.click(screen.getAllByRole('button', { name: /SAY 一段口述/ })[0])
  expect(await screen.findByDisplayValue('后来编辑')).toBeInTheDocument()
  expect(screen.getByRole('button', { name: '确认所选入账' })).toBeDisabled()
})

it('编辑失败后重试保存同一输入，成功后清除缓冲并恢复确认', async () => {
  const fixture = fixtureHandlers()
  let fails = true
  const { ipc } = mountReview({ update_draft: args => {
    if (fails) failure()
    return fixture.handlers.update_draft(args)
  } }, fixture)
  await waitReady()
  const merchant = screen.getByDisplayValue('A-d1')
  fireEvent.change(merchant, { target: { value: '核对后的商户' } }); fireEvent.blur(merchant)
  await screen.findByText(/未保存：合成写入失败/)
  fails = false
  fireEvent.click(screen.getByRole('button', { name: '重试保存' }))
  await waitReady()
  expect(merchant).toHaveValue('核对后的商户')
  expect(screen.queryByRole('button', { name: '重试保存' })).not.toBeInTheDocument()
  expect(ipc.calls.filter(c => c.command === 'update_draft')).toHaveLength(2)
})

it('旧 attempt 的 mutation 完成只重取最新投影，不能把旧草稿带回来', async () => {
  const saved = deferred<unknown>()
  const { state, client } = mountReview({ discard_draft: () => saved.promise })
  await waitReady()
  fireEvent.click(screen.getAllByRole('button', { name: '丢弃' })[0])
  state.sources[0].latestAttemptId = 'A-2'; state.drafts = [draft('new', 'A', 'A-2')]; state.totals = [total('A', 'A-2')]
  await act(async () => { await refreshReview(client, 'A') })
  await screen.findByDisplayValue('new')
  await act(async () => saved.resolve(null))
  await waitReady()
  expect(screen.queryByDisplayValue('A-d1')).not.toBeInTheDocument()
})

it.each(['confirm_draft', 'discard_draft'])('%s 失败后保持草稿并支持重试', async command => {
  let fails = true
  const { ipc } = mountReview({ [command]: () => { if (fails) failure(); return null } })
  await waitReady()
  const label = command === 'confirm_draft' ? '单条确认' : '丢弃'
  fireEvent.click(screen.getAllByRole('button', { name: label })[0])
  await screen.findByText('合成写入失败')
  expect(screen.getByDisplayValue('A-d1')).toBeInTheDocument()
  fails = false; await waitReady()
  fireEvent.click(screen.getAllByRole('button', { name: label })[0])
  await waitFor(() => expect(ipc.calls.filter(c => c.command === command)).toHaveLength(2))
})

it('重复点击只写一次，批量部分 rejected 明确显示', async () => {
  const pending = deferred<unknown>()
  const { ipc } = mountReview({ confirm_drafts: () => pending.promise })
  await waitReady()
  const button = screen.getByRole('button', { name: '确认所选入账' })
  fireEvent.click(button); fireEvent.click(button)
  await act(async () => pending.resolve({ confirmed: [{}], rejected: [{ message: '第2条缺汇率' }] }))
  await screen.findByText(/1 条未确认：第2条缺汇率/)
  expect(ipc.calls.filter(c => c.command === 'confirm_drafts')).toHaveLength(1)
})

it('写入成功但重取失败：只重试读取，绝不重发确认', async () => {
  const fixture = fixtureHandlers()
  let written = false, fails = true
  const { ipc } = mountReview({ confirm_draft: () => { written = true; return null }, list_active_drafts: args => {
    if (written && fails) throw new Error('合成读取失败')
    return fixture.handlers.list_active_drafts(args)
  } }, fixture)
  await waitReady()
  fireEvent.click(screen.getAllByRole('button', { name: '单条确认' })[0])
  await screen.findByText('操作已完成，读取失败。')
  expect(screen.getByRole('button', { name: '确认所选入账' })).toBeDisabled()
  fails = false
  fireEvent.click(screen.getByRole('button', { name: '重试读取' }))
  await waitReady()
  expect(ipc.calls.filter(c => c.command === 'confirm_draft')).toHaveLength(1)
})
