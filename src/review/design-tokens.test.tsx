import { readFileSync } from 'node:fs'
import { cleanup, fireEvent, screen } from '@testing-library/react'
import { clearMocks } from '@tauri-apps/api/mocks'
import { afterEach, expect, it } from 'vitest'
import { formatDifference } from '../lib/money'
import { fixtureHandlers } from '../test/fixtures/limitedM1'
import { mountReview } from '../test/reviewScreen'

afterEach(() => { cleanup(); clearMocks() })
it('semantic 与 design.md 一致，组件禁止字面颜色与 primitive，禁用不造新底色', () => {
  const spec = readFileSync('design.md', 'utf8').split('colors:\n')[1].split('\ntypography:')[0]
  const tokens = readFileSync('src/styles/tokens.css', 'utf8')
  const values = new Map([...tokens.matchAll(/--([\w-]+):\s*([^;]+);/g)].map(match => [match[1], match[2]]))
  function resolve(key: string): string | undefined {
    const value = values.get(key)
    const ref = value?.match(/^var\(--([\w-]+)\)$/)
    return ref ? resolve(ref[1]) : value
  }
  for (const [, key, expected] of spec.matchAll(/^ {2}([\w-]+): "([^"]+)"/gm)) {
    if (/^(fg-|intent-|evidence-|draft-)/.test(key)) expect(resolve(`color-${key}`), key).toBe(expected)
    if (/^(ink|paper|brand|positive|caution|critical)-\d+$/.test(key)) expect(resolve(key), key).toBe(expected)
  }
  const css = readFileSync('src/styles.css', 'utf8')
  expect(css).not.toMatch(/#[0-9a-f]{3,8}\b|rgba?\(|oklch\(/i)
  expect(css).not.toMatch(/var\(--(?:ink|paper|brand|positive|caution|critical)-/)
  expect(css).not.toMatch(/font-size:\s*(?:[0-9]|10)px/)
  expect(css).not.toMatch(/opacity\s*:|bg-disabled/)
  expect(css).toContain('repeating-linear-gradient(45deg')
  expect(css).toContain('prefers-reduced-motion')
  expect(css).toContain('var(--color-draft-marker-picked)')
  expect(css).toContain('.field-inline:focus')
  expect(css).toContain('.field-boxed')
  expect(css).toContain('.composer')
})

it.each([
  [null, 1, 'JPY', '—'], [1, null, 'CNY', '—'], [100, 200, 'JPY', '−100 JPY'],
  [1001, 1000, 'KWD', '0.001 KWD'], [1000000000000000, -1000000000000000, 'KWD', '2000000000000.000 KWD'],
] as const)('差额只格式化两侧整数 %s / %s %s', (a, b, currency, expected) => {
  expect(formatDifference(a, b, currency)).toBe(expected)
})
it('差额拒绝越界与非整数输入', () => {
  expect(() => formatDifference(1e15 + 1, 0, 'CNY')).toThrow()
  expect(() => formatDifference(1.2, 0, 'CNY')).toThrow()
})

it.each(['passed', 'failed', 'unavailable', 'not_applicable'] as const)('口述 %s 保留背书、完整原件、全部结果与确认入口', async status => {
  const fixture = fixtureHandlers()
  fixture.state.totals[0].reconciliationStatus = status
  mountReview({}, fixture)
  await screen.findByDisplayValue('A-d1')
  expect(screen.getByTestId('original-text')).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: '2 条待确认' })).toBeInTheDocument()
  expect(screen.getByRole('note')).toHaveTextContent('确认前请对着原文过一遍')
  expect(screen.getByRole('button', { name: '确认所选入账' })).toBeEnabled()
  expect(screen.getByRole('article', { name: '草稿 1 A-d1' })).toHaveAttribute('tabindex', '0')
})

it.each(['failed', 'unavailable'] as const)('file %s 禁止批量、原件加载后允许合法单条确认', async status => {
  const fixture = fixtureHandlers()
  fixture.state.sources[0].kind = 'file'
  fixture.state.totals[0] = { ...fixture.state.totals[0], sourceKind: 'file', reconciliationStatus: status, confirmationPolicy: 'single_only' }
  mountReview({ read_evidence: () => ({ kind: 'file', mimeType: 'image/png', dataBase64: 'synthetic', text: null }) }, fixture)
  await screen.findByDisplayValue('A-d1')
  expect(screen.getAllByRole('button', { name: '单条确认' })[0]).toBeDisabled()
  fireEvent.load(screen.getByRole('img', { name: '导入的来源原图' }))
  expect(screen.getByRole('button', { name: '确认所选入账' })).toBeDisabled()
  expect(screen.getAllByRole('button', { name: '单条确认' })[0]).toBeEnabled()
})

it('缺三元组与 completed_with_gaps 保留可操作入口和遗漏说明', async () => {
  const fixture = fixtureHandlers()
  fixture.state.drafts[0].baseAmountMinor = null
  fixture.state.totals[0] = { ...fixture.state.totals[0], outcome: 'completed_with_gaps', unparsedNote: '原件底部还有一笔看不清' }
  mountReview({}, fixture)
  await screen.findByText('原件底部还有一笔看不清')
  expect(screen.getByLabelText('本位币')).toHaveClass('field-boxed')
  expect(screen.getByRole('button', { name: '补齐' })).toBeEnabled()
  expect(screen.getAllByRole('button', { name: '单条确认' })[0]).toBeDisabled()
})
