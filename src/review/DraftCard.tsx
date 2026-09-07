import { formatMoney, formatRate } from '../lib/money'
import type { EditBuffer } from './screenReducer'
import type { ReviewDraft } from './types'

interface Props {
  draft: ReviewDraft; selected: boolean; unavailable: boolean; disabled: boolean
  edit?: EditBuffer; defaultBaseCurrency: string
  onToggle: () => void; onFocus: () => void
  onEdit: (field: string, value: string) => void; onSave: (extra?: Record<string, string>) => void; onRestore: () => void
  onConfirm: () => void; onDiscard: () => void
}
export function DraftCard({ draft, selected, unavailable, disabled, edit, defaultBaseCurrency, onToggle, onFocus, onEdit, onSave, onRestore, onConfirm, onDiscard }: Props) {
  const values = edit?.values ?? {}
  const dirty = Object.keys(values).length > 0
  const field = (name: string, fallback: string) => values[name] ?? fallback
  return <article tabIndex={0} aria-label={`草稿 ${draft.sourceOrdinal} ${draft.merchant}`} onFocus={onFocus} onClick={onFocus}
    className={`draft-card ${selected ? 'is-selected' : ''} ${unavailable ? 'has-warning' : ''}`} data-draft-id={draft.id}>
    <button className="draft-check" aria-label={selected ? '取消选择' : '选择'} aria-pressed={selected} onClick={onToggle}>{selected ? '✓' : ''}</button>
    <span className="draft-ordinal">{draft.sourceOrdinal.toString().padStart(2, '0')}</span>
    <div className="draft-fields">
      <input className="merchant-input field-inline" aria-label="商户" value={field('merchant', draft.merchant)} onChange={e => onEdit('merchant', e.target.value)} onBlur={() => onSave()} />
      <div className="amount-line"><input className="field-inline" aria-label="金额" value={field('amount', formatMoney(draft.amountMinor, draft.currency).split(' ')[0].replace('−', '-'))} onChange={e => onEdit('amount', e.target.value)} onBlur={() => onSave()} /><span>{draft.currency}</span></div>
      <div className="draft-meta">
        <input className="field-inline" aria-label="日期" type="date" value={field('occurredOn', draft.occurredOn)} onChange={e => onEdit('occurredOn', e.target.value)} onBlur={() => onSave()} />
        <input className="field-inline" aria-label="分类" value={field('category', draft.category ?? '')} placeholder="未分类" onChange={e => onEdit('category', e.target.value)} onBlur={() => onSave()} />
      </div>
      <p className="evidence-claim"><span>抽取声明</span>“{draft.evidenceText}”</p>
      {draft.baseAmountMinor === null ? <div className="triple-completion">
        <p className="triple-warning">确认前需要补全本位币与汇率</p>
        <input className="field-boxed" aria-label="本位币" maxLength={3} value={field('baseCurrency', defaultBaseCurrency)} onChange={e => onEdit('baseCurrency', e.target.value.toUpperCase())} />
        <input className="field-boxed" aria-label="汇率" placeholder={`1 ${draft.currency} = ?`} value={field('rate', draft.currency === defaultBaseCurrency ? '1' : '')} onChange={e => onEdit('rate', e.target.value)} />
        <button disabled={disabled} onClick={() => onSave({ baseCurrency: field('baseCurrency', defaultBaseCurrency), rate: field('rate', draft.currency === defaultBaseCurrency ? '1' : '') })}>补齐</button>
      </div> : <p className="triple-rate">1 {draft.currency} = {formatRate(draft.ratePpm)} {draft.baseCurrency}</p>}
      {dirty && <div className="edit-status" role="status"><p>未保存{edit?.error ? `：${edit.error}` : ''}</p>
        <button disabled={disabled} onClick={() => onSave()}>重试保存</button><button onClick={onRestore}>恢复已保存值</button>
      </div>}
    </div>
    <div className="draft-card__actions"><button disabled={disabled || dirty || !draft.evidenceText || draft.baseAmountMinor === null} onClick={onConfirm}>单条确认</button><button className="danger" disabled={disabled} onClick={onDiscard}>丢弃</button></div>
  </article>
}
