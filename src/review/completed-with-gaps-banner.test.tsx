import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { CompletedWithGapsBanner } from './CompletedWithGapsBanner'

describe('review/completed-with-gaps-banner', () => {
  it('shows the unparsed note as an alert', () => {
    render(<CompletedWithGapsBanner outcome="completed_with_gaps" note="底部一行被遮挡" />)
    expect(screen.getByRole('alert')).toHaveTextContent('底部一行被遮挡')
  })

  it('does not make ordinary completion look exceptional', () => {
    render(<CompletedWithGapsBanner outcome="completed" note={null} />)
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})
