import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import { QueryClientProvider } from '@tanstack/react-query'
import { createQueryClient } from './lib/queryClient'
import './styles.css'

const root = document.getElementById('root')

if (!root) throw new Error('找不到应用挂载点')

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={createQueryClient()}><App /></QueryClientProvider>
  </StrictMode>,
)
