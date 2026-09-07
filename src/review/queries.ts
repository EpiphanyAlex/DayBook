import { queryOptions, skipToken, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'
import { call } from '../lib/bridge'
import type { BackendStatus, EvidenceContent, FoundationStatus, ReviewDraft, ReviewSource, TotalCheck } from './types'

export const keys = {
  sources: ['review-sources'] as const,
  drafts: (id: string) => ['review-drafts', id] as const,
  evidence: (id: string) => ['review-evidence', id] as const,
  total: (id: string, attempt: string | null) => ['review-total', id, attempt] as const,
  agent: ['agent-status'] as const, logs: ['agent-logs'] as const, foundation: ['foundation-status'] as const,
}
export const sourcesOptions = queryOptions({ queryKey: keys.sources, queryFn: () => call<ReviewSource[]>('list_review_sources') })
export const foundationOptions = queryOptions({ queryKey: keys.foundation, queryFn: () => call<FoundationStatus>('foundation_status') })
export const agentOptions = queryOptions({ queryKey: keys.agent, queryFn: () => call<BackendStatus>('agent_status') })
export const logsOptions = queryOptions({ queryKey: keys.logs, queryFn: () => call<string[]>('recent_agent_logs') })

interface DraftProjection { sourceId: string; attemptId: string | null; drafts: ReviewDraft[] }
export function draftOptions(source: ReviewSource) {
  return queryOptions({ queryKey: keys.drafts(source.id), queryFn: async ({ signal }): Promise<DraftProjection> => {
    const drafts = await call<ReviewDraft[]>('list_active_drafts', { sourceId: source.id })
    // 消费 signal 只丢弃迟到投影，不停止 Rust command。空数组也封装请求身份。
    signal.throwIfAborted()
    if (drafts.some(d => d.sourceId !== source.id || d.attemptId !== source.latestAttemptId)) {
      throw new Error('草稿与当前解析尝试不一致，请重新读取。')
    }
    return { sourceId: source.id, attemptId: source.latestAttemptId, drafts }
  } })
}
export function totalOptions(source: ReviewSource) {
  return queryOptions({ queryKey: keys.total(source.id, source.latestAttemptId), queryFn: async ({ signal }) => {
    if (!source.latestAttemptId) return null
    const check = await call<TotalCheck>('check_source_total', { attemptId: source.latestAttemptId })
    signal.throwIfAborted()
    if (check.sourceId !== source.id || check.attemptId !== source.latestAttemptId || check.sourceKind !== source.kind) {
      throw new Error('合计与当前解析尝试不一致，请重新读取。')
    }
    return check
  } })
}

// disabled observer 只订阅缓存；所有读取由身份变更或明确失效触发，避免旧 queryFn
// 在同一个 source key 换 attempt 的那个 render 里自动启动。
export function useReviewQueries(source: ReviewSource | null) {
  const client = useQueryClient()
  const id = source?.id ?? ''
  const attempt = source?.latestAttemptId ?? null
  const draftsQuery = useQuery({ queryKey: keys.drafts(id), queryFn: source ? draftOptions(source).queryFn : skipToken, enabled: false })
  const totalQuery = useQuery({ queryKey: keys.total(id, attempt), queryFn: source ? totalOptions(source).queryFn : skipToken, enabled: false })
  const evidenceQuery = useQuery({ queryKey: keys.evidence(id), enabled: Boolean(source), queryFn: async ({ signal }) => {
    const content = await call<EvidenceContent>('read_evidence', { sourceId: id })
    signal.throwIfAborted()
    if (content.kind !== source?.kind || (content.kind === 'utterance' ? typeof content.text !== 'string' || !content.text : !content.dataBase64)) {
      throw new Error('原件内容与当前来源不一致，请重新读取。')
    }
    return { sourceId: id, content }
  } })
  useEffect(() => {
    if (!source) return
    void loadReview(client, source).catch(() => { /* query 错误由屏幕显示 */ })
    return () => { void client.cancelQueries({ queryKey: keys.drafts(id), exact: true }) }
    // 查询身份只有 sourceId / attemptId；计数变动由 mutation 定向刷新。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, id, attempt])
  const identityMatches = draftsQuery.data?.sourceId === id && draftsQuery.data.attemptId === attempt
  const ready = identityMatches && draftsQuery.isSuccess && totalQuery.isSuccess && !draftsQuery.isFetching && !totalQuery.isFetching
  return {
    drafts: identityMatches ? draftsQuery.data!.drafts : [],
    check: totalQuery.isSuccess ? totalQuery.data : null,
    evidence: evidenceQuery.isSuccess && evidenceQuery.data.sourceId === id && evidenceQuery.data.content.kind === source?.kind ? evidenceQuery.data.content : null,
    evidenceQuery, ready,
    error: draftsQuery.error ?? totalQuery.error,
  }
}

export async function loadReview(client: QueryClient, source: ReviewSource, force = false) {
  const cached = client.getQueryData<DraftProjection>(keys.drafts(source.id))
  if (force || (cached && cached.attemptId !== source.latestAttemptId)) {
    await client.cancelQueries({ queryKey: keys.drafts(source.id), exact: true })
    await client.invalidateQueries({ queryKey: keys.drafts(source.id), exact: true, refetchType: 'none' })
    await client.invalidateQueries({ queryKey: keys.total(source.id, source.latestAttemptId), exact: true, refetchType: 'none' })
  }
  await Promise.all([client.fetchQuery(draftOptions(source)), client.fetchQuery(totalOptions(source))])
}

export async function refreshSources(client: QueryClient) {
  await client.cancelQueries({ queryKey: keys.sources, exact: true })
  await client.invalidateQueries({ queryKey: keys.sources, exact: true, refetchType: 'none' })
  return client.fetchQuery(sourcesOptions)
}
export async function refreshReview(client: QueryClient, sourceId: string) {
  const sources = await refreshSources(client)
  const source = sources.find(s => s.id === sourceId)
  if (source) await loadReview(client, source, true)
}

// StrictMode / 再挂载不会再次发出能力探测；它是显式动作，不是自动 queryFn。
const probes = new WeakMap<QueryClient, Promise<void>>()
export function probeOnce(client: QueryClient) {
  let promise = probes.get(client)
  if (!promise) {
    promise = call<BackendStatus>('probe_agent').then(async status => {
      await client.cancelQueries({ queryKey: keys.agent, exact: true })
      client.setQueryData(keys.agent, status)
      await client.invalidateQueries({ queryKey: keys.logs })
    })
    probes.set(client, promise)
  }
  return promise
}
