import { formatMoney, formatDifference } from '../lib/money'
import type { ReconciliationStatus } from './policy'
import type { TotalCheck } from './types'

const COPY: Record<ReconciliationStatus, [string, string]> = {
  passed: ['账已对上', '草稿合计与来源声明完全一致'],
  failed: ['差额报警', '草稿合计与来源声明不一致'],
  unavailable: ['无法校验', '来源合计缺失，或有条目无法折算'],
  not_applicable: ['请你背书', '没有可校验的来源合计，请对着全文过一遍'],
}
export function ReconciliationCard({ check }: { check: TotalCheck }) {
  const copy = COPY[check.reconciliationStatus]
  return <aside className={`reconciliation reconciliation--${check.reconciliationStatus}`}>
    <strong>{copy[0]}</strong><p>{copy[1]}</p>
    {check.reportedTotalMinor !== null && <dl>
      <div><dt>来源声明</dt><dd>{formatMoney(check.reportedTotalMinor, check.reportedTotalCurrency)}</dd></div>
      <div><dt>草稿合计</dt><dd>{formatMoney(check.calculatedTotalMinor, check.reportedTotalCurrency)}</dd></div>
      <div><dt>差额（草稿 − 声明）</dt><dd>{formatDifference(check.calculatedTotalMinor, check.reportedTotalMinor, check.reportedTotalCurrency)}</dd></div>
    </dl>}
    {check.reportedTotalEvidenceText && <blockquote>“{check.reportedTotalEvidenceText}”</blockquote>}
    {check.unavailableDraftIds.length > 0 && <p>无法折算：{check.unavailableDraftIds.join('、')}</p>}
  </aside>
}
