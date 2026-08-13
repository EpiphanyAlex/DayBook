import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { App } from './App'

describe('App', () => {
  it('renders the product promise', () => {
    render(<App />)
    expect(screen.getByRole('heading', { name: '把过去的钱和事，补记回来。' })).toBeInTheDocument()
  })
})
