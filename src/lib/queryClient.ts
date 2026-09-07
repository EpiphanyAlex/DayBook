import { QueryClient } from '@tanstack/react-query'

export function createQueryClient() {
  return new QueryClient({ defaultOptions: {
    queries: {
      staleTime: Infinity, retry: false, refetchOnWindowFocus: false,
      refetchOnReconnect: false, refetchOnMount: false, networkMode: 'always',
    },
    mutations: { retry: false, networkMode: 'always' },
  } })
}
