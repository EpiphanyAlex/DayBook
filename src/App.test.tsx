import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { App } from './App'

describe('App', () => {
  it('renders the product promise', () => {
    render(<App />)
    expect(screen.getByText('你的个人事务助理')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: '把零散的钱和事，整理清楚。' })).toBeInTheDocument()
    expect(screen.getByText('拖入截图，或说一段话。不用逐条填表。')).toBeInTheDocument()
  })
})
