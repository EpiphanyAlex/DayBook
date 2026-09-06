import { StrictMode } from 'react'
import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import { App } from '../App'
import { createQueryClient } from '../lib/queryClient'
import { fixtureHandlers } from './fixtures/limitedM1'
import { installMockIpc, type IpcHandler } from './mockIpc'

export function mountReview(overrides: Record<string, IpcHandler> = {}, fixture = fixtureHandlers()) {
  const ipc = installMockIpc({ ...fixture.handlers, ...overrides })
  const client = createQueryClient()
  const view = render(<StrictMode><QueryClientProvider client={client}><App /></QueryClientProvider></StrictMode>)
  return { ...fixture, client, ipc, ...view }
}
export async function waitReady() {
  await waitFor(() => {
    if (screen.getByRole('button', { name: '确认所选入账' }).hasAttribute('disabled')) throw new Error('尚未就绪')
  })
}
