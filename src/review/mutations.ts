import { useRef } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { call } from '../lib/bridge'
import { keys, refreshReview } from './queries'
import type { ReviewSource } from './types'

interface ReviewWrite { command: string; args: Record<string, unknown>; sourceId: string; attemptId: string }
export function useReviewMutation() {
  const client = useQueryClient()
  const lock = useRef(false)
  const mutation = useMutation({ mutationFn: async (write: ReviewWrite) => {
    const source = client.getQueryData<ReviewSource[]>(keys.sources)?.find(s => s.id === write.sourceId)
    if (source?.latestAttemptId !== write.attemptId) throw new Error('解析尝试已改变，请重新核对当前草稿。')
    const result = await call<unknown>(write.command, write.args)
    try {
      await refreshReview(client, write.sourceId)
      return { result, readFailed: false }
    } catch {
      return { result, readFailed: true }
    }
  } })
  async function execute(write: ReviewWrite) {
    if (lock.current) return null
    lock.current = true
    try { return await mutation.mutateAsync(write) } finally { lock.current = false }
  }
  return { execute, pending: mutation.isPending }
}
