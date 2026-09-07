import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClientProvider } from '@tanstack/react-query'
import { App } from '../../App'
import { createQueryClient } from '../../lib/queryClient'
import type { MinorUnits } from '../../lib/bridge'
import { fixtureHandlers, draft } from '../fixtures/limitedM1'
import { installMockIpc } from '../mockIpc'
import '../../styles.css'

// 独立开发入口；生产 index.html 不引用此文件。所有 IPC 均在挂载前封闭。
const scenario = new URLSearchParams(location.search).get('scenario') ?? 'review'
const { state, handlers } = fixtureHandlers()
if (scenario === 'empty') {
  state.sources = []
}
state.drafts[0].merchant = '街角咖啡'
state.drafts[1] = { ...state.drafts[1], merchant: '午间食堂', sourceOrdinal: 2, evidenceText: '午饭24元', evidenceSpanStart: 9, evidenceSpanEnd: 14, amountMinor: 2400 as MinorUnits, baseAmountMinor: 2400 as MinorUnits }
if (scenario === 'parsing') { state.sources[0].state = 'parsing'; state.drafts = []; state.sources[0].activeDraftCount = 0 }
if (scenario === 'failed') {
  state.totals[0] = { ...state.totals[0], reconciliationStatus: 'failed', reportedTotalMinor: 4200 as MinorUnits, calculatedTotalMinor: 3600 as MinorUnits, reportedTotalCurrency: 'CNY', reportedTotalEvidenceText: '总共42元' }
  state.drafts[1] = { ...state.drafts[1], amountMinor: 1800 as MinorUnits, baseAmountMinor: 1800 as MinorUnits }
}
const canvas = document.createElement('canvas')
canvas.width = 700; canvas.height = 650
const ctx = canvas.getContext('2d')!
ctx.fillStyle = '#fffdf8'; ctx.fillRect(0, 0, 700, 650)
ctx.fillStyle = '#211208'; ctx.font = '28px monospace'; ctx.fillText('DAYBOOK · SYNTHETIC RECEIPT', 48, 72)
ctx.font = '22px monospace'
for (const [index, line] of ['2026-09-05', 'Coffee                  CNY 18.00', 'Lunch                   CNY 24.00', '', 'TOTAL SPENT             CNY 42.00', '', 'Test fixture · no personal data'].entries()) ctx.fillText(line, 48, 140 + index * 60)
const imageBase64 = canvas.toDataURL('image/png').split(',')[1]
if (scenario.startsWith('file-')) {
  state.sources[0] = { ...state.sources[0], kind: 'file', ext: 'png', originalFilename: '合成收据.png' }
  const status = scenario === 'file-passed' ? 'passed' : scenario === 'file-failed' ? 'failed' : 'unavailable'
  state.totals[0] = { ...state.totals[0], sourceKind: 'file', reconciliationStatus: status, confirmationPolicy: status === 'passed' ? 'reconciled_batch' : 'single_only', reportedTotalMinor: status === 'unavailable' ? null : 4200 as MinorUnits, calculatedTotalMinor: 3600 as MinorUnits, reportedTotalCurrency: 'CNY', reportedTotalEvidenceText: status === 'unavailable' ? null : 'TOTAL SPENT CNY 42.00' }
  if (status === 'passed') state.totals[0].calculatedTotalMinor = 4200 as MinorUnits
  else { state.drafts[1].amountMinor = 1800 as MinorUnits; state.drafts[1].baseAmountMinor = 1800 as MinorUnits }
}
if (scenario === 'gaps') {
  state.totals[0] = { ...state.totals[0], outcome: 'completed_with_gaps', unparsedNote: '末尾提到的一笔车费缺少金额，请对照原件补记。' }
  state.drafts[0].baseAmountMinor = null; state.drafts[0].baseCurrency = null; state.drafts[0].ratePpm = null
}
const longText = Array.from({ length: 8 }, (_, i) => `第${i + 1}天：🧾午饭18元。`).join('\n')
if (scenario === 'long') {
  state.sources[0].activeDraftCount = 8
  state.drafts = Array.from({ length: 8 }, (_, i) => ({ ...draft(`long-${i}`), merchant: `第${i + 1}天午饭`, sourceOrdinal: i + 1, evidenceText: '🧾午饭18元', evidenceSpanStart: i * 12 + 4, evidenceSpanEnd: i * 12 + 10 }))
}
installMockIpc({ ...handlers,
  read_evidence: () => scenario.startsWith('file-') ? { kind: 'file', mimeType: 'image/png', dataBase64: imageBase64, text: null } : { ...handlers.read_evidence(), text: scenario === 'long' ? longText : handlers.read_evidence().text + (scenario === 'failed' ? '总共42元。' : scenario === 'gaps' ? '另外坐车回家，金额记不清了。' : '') },
  probe_agent: () => scenario === 'empty' ? new Promise(() => {}) : handlers.probe_agent(),
  foundation_status: () => ({ ...handlers.foundation_status(), baseCurrency: scenario === 'empty' ? null : 'CNY' }),
  agent_status: () => ({ ...handlers.agent_status(), ready: scenario !== 'empty' }),
  parse_source: () => new Promise(() => {}),
  cancel_parsing: () => { state.sources[0].state = 'failed'; return true },
})
createRoot(document.getElementById('root')!).render(<StrictMode><QueryClientProvider client={createQueryClient()}><App /></QueryClientProvider></StrictMode>)
