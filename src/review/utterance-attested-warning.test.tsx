import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { AttestationHint } from './AttestationHint'

describe('review/utterance-attested-warning', () => {
  const totals = { reportedTotalText: '50.00 AUD', calculatedTotalText: '1.00 AUD' }

  it('keeps the attestation prompt on screen for every user-attested outcome', () => {
    for (const reconciliationStatus of ['passed', 'failed', 'not_applicable'] as const) {
      const { unmount } = render(
        <AttestationHint
          policy={{ reconciliationStatus, confirmationPolicy: 'user_attested_batch' }}
          {...totals}
        />,
      )
      expect(screen.getByRole('note')).toHaveTextContent('确认前请对着原文过一遍')
      unmount()
    }
  })

  it('puts the difference in front of the user when the machine says it does not add up', () => {
    render(
      <AttestationHint
        policy={{ reconciliationStatus: 'failed', confirmationPolicy: 'user_attested_batch' }}
        {...totals}
      />,
    )
    const note = screen.getByRole('note')
    expect(note).toHaveTextContent('50.00 AUD')
    expect(note).toHaveTextContent('1.00 AUD')
    expect(note).toHaveTextContent('由你背书')
  })

  it('does not claim a passed reconciliation is a mismatch', () => {
    render(
      <AttestationHint
        policy={{ reconciliationStatus: 'passed', confirmationPolicy: 'user_attested_batch' }}
        {...totals}
      />,
    )
    expect(screen.getByRole('note')).not.toHaveTextContent('合计对不上')
  })

  it('stays out of the way when batch confirmation is not user-attested', () => {
    for (const confirmationPolicy of ['single_only', 'reconciled_batch'] as const) {
      const { unmount } = render(
        <AttestationHint
          policy={{ reconciliationStatus: 'failed', confirmationPolicy }}
          {...totals}
        />,
      )
      expect(screen.queryByRole('note')).not.toBeInTheDocument()
      unmount()
    }
  })
})
