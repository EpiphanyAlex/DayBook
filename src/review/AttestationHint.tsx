import type { ReviewPolicy } from './policy'

interface AttestationHintProps {
  policy: ReviewPolicy
  reportedTotalText: string | null
  calculatedTotalText: string | null
}

/**
 * `user_attested_batch` 那一档的背书提示，必须与批量确认按钮同屏（03 §3.4 第 2 档）。
 *
 * `failed` 是全产品唯一一条「机器已判定对不上却仍允许批量」的路径——差额要和按钮一起看得见，
 * 否则机器那道闸门判了没人看，人那道因为用户不知情而没有真的发生。
 */
export function AttestationHint({
  policy,
  reportedTotalText,
  calculatedTotalText,
}: AttestationHintProps) {
  if (policy.confirmationPolicy !== 'user_attested_batch') return null
  const mismatch = policy.reconciliationStatus === 'failed'
  return (
    <aside className={`attestation-hint ${mismatch ? 'is-mismatch' : ''}`} role="note">
      {mismatch && (
        <strong>
          合计对不上 · 来源声明 {reportedTotalText ?? '—'} · 草稿合计 {calculatedTotalText ?? '—'}
        </strong>
      )}
      <p>
        {mismatch
          ? '批量确认仍然可用，但这一下由你背书 —— 确认前请对着原文过一遍。'
          : '确认前请对着原文过一遍。'}
      </p>
    </aside>
  )
}
