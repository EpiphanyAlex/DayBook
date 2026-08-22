import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import iconUrl from '../assets/brand/icon.svg'
import { backendPresentation } from './agent/presentation'
import { runQueueContinuing } from './ingest/queue'
import { AppError, call, type MinorUnits, type RatePpm } from './lib/bridge'
import { formatMoney, formatRate, parseMoneyInput, parseRateInput } from './lib/money'
import { AttestationHint } from './review/AttestationHint'
import { CompletedWithGapsBanner } from './review/CompletedWithGapsBanner'
import {
  canBatchConfirm,
  POLICY,
  RECONCILIATION,
  SOURCE_STATE,
  type ReconciliationStatus,
  type ReviewPolicy,
  type SourceStateValue,
  type UtteranceGateState,
} from './review/policy'

interface FoundationStatus {
  schemaVersion: number
  dataDirectory: string
  baseCurrency: string | null
  debugLogging: boolean
}

interface BackendStatus {
  available: boolean
  availabilityReason: string | null
  authenticated: boolean | null
  ready: boolean
  errorCode: string | null
  version: string | null
}

interface ReviewSource {
  id: string
  kind: 'file' | 'utterance'
  originalFilename: string | null
  ext: string
  state: SourceStateValue
  parseErrorCode: string | null
  latestAttemptId: string | null
  importedAt: string
  activeDraftCount: number
}

interface ReviewDraft {
  id: string
  sourceId: string
  attemptId: string
  evidenceText: string
  sourceOrdinal: number
  evidenceSpanStart: number | null
  evidenceSpanEnd: number | null
  occurredOn: string
  amountMinor: MinorUnits
  currency: string
  baseAmountMinor: MinorUnits | null
  baseCurrency: string | null
  ratePpm: RatePpm | null
  direction: 'expense' | 'income' | 'transfer'
  merchant: string
  category: string | null
  channel: string | null
  confidence: number | null
}

interface TotalCheck extends ReviewPolicy {
  attemptId: string
  sourceId: string
  sourceKind: string
  reportedTotalMinor: MinorUnits | null
  calculatedTotalMinor: MinorUnits | null
  reportedTotalCurrency: string | null
  reportedTotalKind: string | null
  reportedTotalEvidenceText: string | null
  unavailableDraftIds: string[]
  outcome: string | null
  unparsedNote: string | null
}

interface EvidenceContent {
  kind: 'file' | 'utterance'
  mimeType: string
  dataBase64: string | null
  text: string | null
}

interface ImportResult {
  sourceId: string
  deduplicated: boolean
  matchingUtteranceSourceIds: string[]
}

const isTauri = () => '__TAURI_INTERNALS__' in window

function sourceTitle(source: ReviewSource): string {
  if (source.kind === 'utterance') return '一段口述'
  return source.originalFilename || `图片 · ${source.ext.toUpperCase()}`
}

function sourceStateLabel(source: ReviewSource): string {
  const labels: Record<SourceStateValue, string> = {
    imported: '等待解析',
    parsing: '正在还原',
    parsed: source.activeDraftCount ? `${source.activeDraftCount} 条待确认` : '等待审核',
    failed: '解析失败',
    reviewed: '已归档',
  }
  return labels[source.state]
}

function errorMessage(error: unknown): string {
  return error instanceof AppError ? error.message : '本地操作没有完成，请重试。'
}

export function App() {
  const [foundation, setFoundation] = useState<FoundationStatus | null>(null)
  const [baseCurrencyInput, setBaseCurrencyInput] = useState('')
  const [backend, setBackend] = useState<BackendStatus | null>(null)
  const [sources, setSources] = useState<ReviewSource[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [drafts, setDrafts] = useState<ReviewDraft[]>([])
  const [selectedDrafts, setSelectedDrafts] = useState<Set<string>>(new Set())
  const [evidence, setEvidence] = useState<EvidenceContent | null>(null)
  const [check, setCheck] = useState<TotalCheck | null>(null)
  const [utterance, setUtterance] = useState('')
  const [notice, setNotice] = useState<string | null>(null)
  const [agentLogs, setAgentLogs] = useState<string[]>([])
  const [busy, setBusy] = useState(false)
  const submitToken = useRef<string | null>(null)

  const selectedSource = useMemo(
    () => sources.find((source) => source.id === selectedId) ?? null,
    [selectedId, sources],
  )

  // 返回刚取到的那一份：`setSources` 之后本轮闭包里的 `sources` 仍是旧值，
  // 调用方要用新数据必须拿返回值，不能读 state。
  const refreshSources = useCallback(async (preferId?: string): Promise<ReviewSource[]> => {
    const next = await call<ReviewSource[]>('list_review_sources')
    setSources(next)
    setSelectedId((current) => {
      if (preferId && next.some((source) => source.id === preferId)) return preferId
      if (current && next.some((source) => source.id === current)) return current
      return next[0]?.id ?? null
    })
    return next
  }, [])

  const refreshAgentLogs = useCallback(async () => {
    setAgentLogs(await call<string[]>('recent_agent_logs'))
  }, [])

  const refreshSelected = useCallback(async (source: ReviewSource) => {
    const [nextDrafts, nextEvidence, nextCheck] = await Promise.all([
      call<ReviewDraft[]>('list_active_drafts', { sourceId: source.id }),
      call<EvidenceContent>('read_evidence', { sourceId: source.id }),
      source.latestAttemptId
        ? call<TotalCheck>('check_source_total', { attemptId: source.latestAttemptId })
        : Promise.resolve(null),
    ])
    setDrafts(nextDrafts)
    setSelectedDrafts(new Set(nextDrafts.map((draft) => draft.id)))
    setEvidence(nextEvidence)
    setCheck(nextCheck)
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    void Promise.all([
      call<FoundationStatus>('foundation_status').then((status) => {
        setFoundation(status)
        setBaseCurrencyInput(status.baseCurrency ?? '')
      }),
      call<BackendStatus>('agent_status').then(setBackend),
      refreshSources(),
    ]).catch((error) => setNotice(errorMessage(error)))
    // **状态只有一个出处**：`probe_agent` 无论成败都返回运行时持有的那份结论（01 §3.5）。
    // 这里此前在 catch 里把错误码写回自己手上那份 status——前端成了第二个事实源。
    void call<BackendStatus>('probe_agent')
      .then((status) => {
        setBackend(status)
        void refreshAgentLogs()
      })
      .catch((error: unknown) => setNotice(errorMessage(error)))
  }, [refreshAgentLogs, refreshSources])

  useEffect(() => {
    if (!selectedSource || !isTauri()) {
      setDrafts([])
      setEvidence(null)
      setCheck(null)
      return
    }
    void refreshSelected(selectedSource).catch((error) => setNotice(errorMessage(error)))
  }, [refreshSelected, selectedSource])

  const parseImported = useCallback(async (result: ImportResult) => {
    if (result.deduplicated) {
      setNotice('这份来源已经导入过，已为你定位到原记录。')
      await refreshSources(result.sourceId)
      return
    }
    await refreshSources(result.sourceId)
    // 就绪度没建立就不下发解析（01 §3.5）。**导入本身照常完成**——证据已落盘，
    // 等状态变绿再解析即可；服务端也会拒，这里只是别让用户白等一个必然失败的请求。
    if (!backend?.ready) {
      setNotice('来源已安全保存。解析器还没就绪，等状态变成「考古员已就绪」后点「解析」。')
      return
    }
    setNotice('来源已安全保存，正在还原里面的交易。')
    try {
      await call('parse_source', { sourceId: result.sourceId })
      setNotice('解析完成，请对照原件确认。')
    } finally {
      await Promise.all([refreshSources(result.sourceId), refreshAgentLogs()])
    }
  }, [backend, refreshAgentLogs, refreshSources])

  const importPaths = useCallback(async (paths: string[]) => {
    if (!paths.length || busy) return
    setBusy(true)
    const result = await runQueueContinuing(paths, async (path) => {
        const result = await call<ImportResult>('import_source_file', { path })
        await parseImported(result)
        return result.sourceId
    })
    if (result.failures.length) {
      const first = result.failures[0]
      setNotice(`${result.failures.length} 份来源未完成（${first ? errorMessage(first.error) : '未知错误'}）；其余来源已继续处理。`)
    }
    setBusy(false)
  }, [busy, parseImported])

  useEffect(() => {
    if (!isTauri()) return
    let unlisten: (() => void) | undefined
    void getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'drop') void importPaths(event.payload.paths)
    }).then((dispose) => { unlisten = dispose })
    return () => unlisten?.()
  }, [importPaths])

  const submitUtterance = async () => {
    if (!utterance.trim() || busy) return
    setBusy(true)
    submitToken.current ??= crypto.randomUUID()
    try {
      const result = await call<ImportResult>('submit_utterance', {
        text: utterance,
        idempotencyKey: submitToken.current,
      })
      if (result.matchingUtteranceSourceIds.length) {
        setNotice('你以前说过相同的话；这次仍作为一份新来源保留。')
      }
      submitToken.current = null
      setUtterance('')
      await parseImported(result)
    } catch (error) {
      setNotice(errorMessage(error))
    } finally {
      setBusy(false)
    }
  }

  const saveBaseCurrency = async () => {
    const currency = baseCurrencyInput.trim().toUpperCase()
    if (!currency) return
    try {
      const status = await call<FoundationStatus>('set_base_currency', { currency })
      setFoundation(status)
      setBaseCurrencyInput(status.baseCurrency ?? '')
      setNotice(`之后的新解析将使用 ${currency} 作为本位币；已有记录不会改变。`)
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const toggleDebugLogging = async () => {
    if (!foundation) return
    try {
      const status = await call<FoundationStatus>('set_debug_logging', { enabled: !foundation.debugLogging })
      setFoundation(status)
      await refreshAgentLogs()
      setNotice(status.debugLogging
        ? '详细调试日志已开启，会在本机记录完整账目细节，14 天后自动清除。'
        : '详细调试日志已关闭；不含账目内容的 trace 仍会保留 14 天。')
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const revealDataDirectory = async () => {
    try {
      await call('reveal_data_directory')
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const retrySource = async (sourceId: string) => {
    if (!backend?.ready) {
      setNotice('解析器还没就绪，等状态变成「考古员已就绪」后再解析。')
      return
    }
    setBusy(true)
    try {
      await call('parse_source', { sourceId })
      setNotice('重新解析完成。')
    } catch (error) {
      setNotice(errorMessage(error))
    } finally {
      await Promise.all([refreshSources(sourceId), refreshAgentLogs()])
      setBusy(false)
    }
  }

  const stopParsing = async () => {
    try {
      const stopped = await call<boolean>('cancel_parsing')
      setNotice(stopped ? '解析已停止；本次产生的草稿已作废，可以安全重试。' : '当前没有正在运行的解析。')
      await Promise.all([refreshSources(selectedId ?? undefined), refreshAgentLogs()])
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const afterReviewAction = async () => {
    if (!selectedSource) return
    const next = await refreshSources(selectedSource.id)
    const latest = next.find((source) => source.id === selectedSource.id)
    if (latest) await refreshSelected(latest)
  }

  const confirmOne = async (draftId: string) => {
    try {
      await call('confirm_draft', { draftId })
      setNotice('已确认 1 条交易。')
      await afterReviewAction()
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const gateState: UtteranceGateState = {
    fullSourceVisible: selectedSource?.kind === 'utterance' && typeof evidence?.text === 'string',
    resultsAdjacent: drafts.length > 0,
    itemCountVisible: drafts.length > 0,
  }
  const selectedList = drafts.filter((draft) => selectedDrafts.has(draft.id))
  const batchReady = Boolean(check && selectedList.length && canBatchConfirm(check, gateState))

  const confirmSelected = async () => {
    if (!batchReady || !check) return
    try {
      const result = await call<{ confirmed: unknown[]; rejected: { message: string }[] }>(
        'confirm_drafts',
        {
          draftIds: selectedList.map((draft) => draft.id),
          attestation: check.confirmationPolicy === POLICY.USER_ATTESTED_BATCH ? gateState : null,
        },
      )
      setNotice(result.rejected.length
        ? `已确认 ${result.confirmed.length} 条；${result.rejected.length} 条需要补全后再确认。`
        : `已确认 ${result.confirmed.length} 条交易。`)
      await afterReviewAction()
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const discard = async (draftId: string) => {
    try {
      await call('discard_draft', { draftId })
      setNotice('这条草稿已丢弃，原始记录仍保留在审计中。')
      await afterReviewAction()
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const patchDraft = async (draftId: string, patch: Record<string, unknown>) => {
    try {
      await call('update_draft', { draftId, patch })
      if (selectedSource) await refreshSelected(selectedSource)
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const agentView = backendPresentation(backend)

  return (
    <main className="workbench">
      <header className="topbar">
        <div className="brand-lockup">
          <img src={iconUrl} alt="" />
          <div><strong>日簿</strong><span>DAYBOOK</span></div>
        </div>
        <div className={`agent-pill ${agentView.ready ? 'is-ready' : 'is-offline'}`}>
          <span aria-hidden="true" />
          {agentView.label}
        </div>
        <p className="privacy-note">账本与证据留在这台 Mac；解析内容会交给你选择的 CLI。</p>
      </header>

      <div className="desk">
        <aside className="source-drawer" aria-label="来源">
          <div className="drawer-heading">
            <p>来源夹</p>
            <span>{sources.length.toString().padStart(2, '0')}</span>
          </div>

          {agentView.title && (
            <aside className="agent-guidance" aria-live="polite">
              <strong>{agentView.title}</strong>
              <p>{agentView.instruction}</p>
            </aside>
          )}

          <form className="base-currency-setting" onSubmit={(event) => { event.preventDefault(); void saveBaseCurrency() }}>
            <label htmlFor="base-currency">当前本位币</label>
            <div>
              <input
                id="base-currency"
                list="currency-suggestions"
                maxLength={3}
                value={baseCurrencyInput}
                placeholder="例如 AUD"
                onChange={(event) => setBaseCurrencyInput(event.target.value.toUpperCase())}
              />
              <datalist id="currency-suggestions">
                {['AUD', 'CNY', 'USD', 'EUR', 'GBP', 'JPY', 'HKD', 'NZD', 'SGD', 'CAD'].map((currency) => <option key={currency} value={currency} />)}
              </datalist>
              <button type="submit" disabled={baseCurrencyInput.length !== 3}>设定</button>
            </div>
            {!foundation?.baseCurrency && <small>首次解析前请明确选择；不会按地区猜测。</small>}
          </form>

          {foundation && (
            <button className={`debug-toggle ${foundation.debugLogging ? 'is-on' : ''}`} onClick={() => void toggleDebugLogging()}>
              <span aria-hidden="true" />
              <span><strong>详细调试日志</strong><small>{foundation.debugLogging ? '已开 · 会记录完整账目细节' : '已关 · trace 不含账目内容'}</small></span>
            </button>
          )}

          <section className="drop-pocket" aria-label="拖入来源">
            <span className="drop-pocket__glyph" aria-hidden="true">↘</span>
            <strong>{busy ? '正在处理…' : '把截图拖到这里'}</strong>
            <small>PNG 或 JPEG · 原件逐位保存</small>
          </section>

          <form className="utterance-box" onSubmit={(event) => { event.preventDefault(); void submitUtterance() }}>
            <label htmlFor="utterance">或者，说一段过去的事</label>
            <textarea
              id="utterance"
              value={utterance}
              onChange={(event) => setUtterance(event.target.value)}
              placeholder="昨天午饭 18，回家打车 24…"
              rows={4}
            />
            <button type="submit" disabled={!utterance.trim() || busy}>保存并拆分</button>
          </form>

          <nav className="source-list" aria-label="已导入来源">
            {sources.map((source) => (
              // 两个按钮是兄弟，不是嵌套：`<button>` 套 `<button>`（或套 role="button"）
              // 在读屏与键盘导航下都不成立，而原来的 span 还只响应 Enter、不响应 Space。
              <div
                key={source.id}
                className={`source-ticket ${source.id === selectedId ? 'is-selected' : ''}`}
              >
                <button
                  type="button"
                  className="source-ticket__main"
                  aria-current={source.id === selectedId || undefined}
                  onClick={() => setSelectedId(source.id)}
                >
                  <span className="source-ticket__kind">{source.kind === 'file' ? 'IMG' : 'SAY'}</span>
                  <span className="source-ticket__copy">
                    <strong>{sourceTitle(source)}</strong>
                    <small className={`state-${source.state}`}>{sourceStateLabel(source)}</small>
                  </span>
                </button>
                {(source.state === SOURCE_STATE.FAILED || source.state === SOURCE_STATE.IMPORTED) && (
                  <button
                    type="button"
                    className="retry-link"
                    disabled={!agentView.ready}
                    title={agentView.ready ? undefined : '解析器还没就绪'}
                    onClick={() => void retrySource(source.id)}
                  >{source.state === SOURCE_STATE.FAILED ? '重试' : '解析'}</button>
                )}
                {source.state === SOURCE_STATE.PARSING && (
                  <button
                    type="button"
                    className="retry-link stop-link"
                    onClick={() => void stopParsing()}
                  >停止</button>
                )}
              </div>
            ))}
          </nav>

          <details className="agent-log-panel">
            <summary>本机解析日志 · 最近 {agentLogs.length} 条</summary>
            {agentLogs.length ? (
              <ol>{agentLogs.map((line, index) => <li key={`${index}-${line}`}>{line}</li>)}</ol>
            ) : <p>完成一次能力检查或解析后，这里会出现排障记录。</p>}
            <button type="button" onClick={() => void refreshAgentLogs()}>刷新日志</button>
          </details>

          {foundation && (
            <aside className="data-location">
              <strong>本地数据目录</strong>
              <code>{foundation.dataDirectory}</code>
              <button type="button" onClick={() => void revealDataDirectory()}>在访达中显示</button>
            </aside>
          )}
        </aside>

        <section className="evidence-stage" aria-label="来源原件">
          {selectedSource ? (
            <>
              <header className="stage-heading">
                <div>
                  <p>来源原件 · 不可改写</p>
                  <h1>{sourceTitle(selectedSource)}</h1>
                </div>
                <span>{selectedSource.kind === 'file' ? selectedSource.ext.toUpperCase() : '全文转写'}</span>
              </header>
              <div className={`light-table light-table--${selectedSource.kind}`}>
                <span className="measure measure--top" aria-hidden="true" />
                <span className="measure measure--side" aria-hidden="true" />
                {evidence?.dataBase64 && (
                  <img src={`data:${evidence.mimeType};base64,${evidence.dataBase64}`} alt="导入的来源原图" />
                )}
                {evidence?.text !== null && evidence?.text !== undefined && (
                  <pre>{evidence.text}</pre>
                )}
                {!evidence && <p className="stage-loading">正在取出原件…</p>}
              </div>
              {foundation && <p className="data-footnote">本地证据库 · schema {foundation.schemaVersion}</p>}
            </>
          ) : (
            <div className="empty-evidence">
              <img src={iconUrl} alt="" />
              <p>你的个人事务助理</p>
              <h1>把零散的钱和事，整理清楚。</h1>
              <span>拖入截图，或说一段话。不用逐条填表。</span>
            </div>
          )}
        </section>

        <section className="review-tray" aria-label="待确认草稿">
          <header className="review-heading">
            <div><p>拆分结果</p><h2>{drafts.length} 条待确认</h2></div>
            {drafts.length > 0 && (
              <button className="select-all" onClick={() => setSelectedDrafts(
                selectedDrafts.size === drafts.length ? new Set() : new Set(drafts.map((draft) => draft.id)),
              )}>{selectedDrafts.size === drafts.length ? '取消全选' : '全选'}</button>
            )}
          </header>

          <CompletedWithGapsBanner outcome={check?.outcome ?? null} note={check?.unparsedNote ?? null} />
          {check && <ReconciliationCard check={check} />}

          <div className="draft-stack">
            {drafts.map((draft) => (
              <DraftCard
                key={draft.id}
                draft={draft}
                selected={selectedDrafts.has(draft.id)}
                unavailable={check?.unavailableDraftIds.includes(draft.id) ?? false}
                onToggle={() => setSelectedDrafts((current) => {
                  const next = new Set(current)
                  if (next.has(draft.id)) next.delete(draft.id); else next.add(draft.id)
                  return next
                })}
                onPatch={(next) => void patchDraft(draft.id, next)}
                onConfirm={() => void confirmOne(draft.id)}
                onDiscard={() => void discard(draft.id)}
                defaultBaseCurrency={foundation?.baseCurrency ?? ''}
              />
            ))}
            {selectedSource && !drafts.length && (
              <div className="empty-drafts">
                <p>{selectedSource.state === SOURCE_STATE.PARSING ? '正在辨认原件里的交易…' : '这里还没有待确认条目。'}</p>
                {selectedSource.parseErrorCode && <small>{selectedSource.parseErrorCode}</small>}
              </div>
            )}
          </div>

          <footer className="review-actions">
            <div>
              <strong>{selectedDrafts.size}</strong>
              <span>条已选</span>
            </div>
            <button disabled={!batchReady} onClick={() => void confirmSelected()}>
              确认所选入账
            </button>
            {check && (
              <AttestationHint
                policy={check}
                reportedTotalText={formatMoney(check.reportedTotalMinor, check.reportedTotalCurrency)}
                calculatedTotalText={formatMoney(check.calculatedTotalMinor, check.reportedTotalCurrency)}
              />
            )}
            {check?.confirmationPolicy === POLICY.SINGLE_ONLY && (
              <small>{check.reconciliationStatus === RECONCILIATION.FAILED ? '合计不符，只能逐条确认' : '无法核对合计，只能逐条确认'}</small>
            )}
          </footer>
        </section>
      </div>
      {notice && <button className="notice" onClick={() => setNotice(null)}>{notice}<span>关闭</span></button>}
    </main>
  )
}

const RECONCILIATION_COPY: Record<ReconciliationStatus, [string, string]> = {
  passed: ['账已对上', '机器合计与来源声明完全一致'],
  failed: ['差额报警', '草稿合计与来源声明不一致'],
  unavailable: ['无法校验', '来源合计缺失，或有条目无法折算'],
  not_applicable: ['请你背书', '口述本身没有结构化合计，请对着全文过一遍'],
}

function ReconciliationCard({ check }: { check: TotalCheck }) {
  // Record<ReconciliationStatus, …>：将来加第五态时这里会直接编译失败，而不是渲染 undefined。
  const copy = RECONCILIATION_COPY[check.reconciliationStatus]
  return (
    <aside className={`reconciliation reconciliation--${check.reconciliationStatus}`}>
      <div className="reconciliation__status"><span aria-hidden="true" /> <strong>{copy[0]}</strong></div>
      <p>{copy[1]}</p>
      {check.reportedTotalMinor !== null && (
        <dl>
          <div><dt>来源声明</dt><dd>{formatMoney(check.reportedTotalMinor, check.reportedTotalCurrency)}</dd></div>
          <div><dt>草稿合计</dt><dd>{formatMoney(check.calculatedTotalMinor, check.reportedTotalCurrency)}</dd></div>
        </dl>
      )}
      {check.reportedTotalEvidenceText && <blockquote>“{check.reportedTotalEvidenceText}”</blockquote>}
    </aside>
  )
}

interface DraftCardProps {
  draft: ReviewDraft
  selected: boolean
  unavailable: boolean
  onToggle: () => void
  onPatch: (patch: Record<string, unknown>) => void
  onConfirm: () => void
  onDiscard: () => void
  defaultBaseCurrency: string
}

function DraftCard({ draft, selected, unavailable, onToggle, onPatch, onConfirm, onDiscard, defaultBaseCurrency }: DraftCardProps) {
  const [amount, setAmount] = useState(formatMoney(draft.amountMinor, draft.currency).split(' ')[0])
  const [baseCurrency, setBaseCurrency] = useState(defaultBaseCurrency)
  const [rate, setRate] = useState(() => (draft.currency === defaultBaseCurrency ? '1' : ''))
  useEffect(() => setAmount(formatMoney(draft.amountMinor, draft.currency).split(' ')[0]), [draft.amountMinor, draft.currency])
  return (
    <article className={`draft-card ${selected ? 'is-selected' : ''} ${unavailable ? 'has-warning' : ''}`}>
      <button className="draft-check" aria-label={selected ? '取消选择' : '选择'} onClick={onToggle}>
        {selected ? '✓' : ''}
      </button>
      <span className="draft-ordinal">{draft.sourceOrdinal.toString().padStart(2, '0')}</span>
      <div className="draft-fields">
        <input
          className="merchant-input"
          aria-label="商户"
          defaultValue={draft.merchant}
          onBlur={(event) => event.target.value !== draft.merchant && onPatch({ merchant: event.target.value })}
        />
        <div className="amount-line">
          <input
            aria-label="金额"
            value={amount}
            onChange={(event) => setAmount(event.target.value)}
            onBlur={() => {
              const minor = parseMoneyInput(amount, draft.currency)
              if (minor !== null && minor !== draft.amountMinor) onPatch({ amountMinor: minor })
            }}
          />
          <span>{draft.currency}</span>
        </div>
        <div className="draft-meta">
          <input aria-label="日期" type="date" defaultValue={draft.occurredOn} onBlur={(event) => onPatch({ occurredOn: event.target.value })} />
          <input aria-label="分类" defaultValue={draft.category ?? ''} placeholder="未分类" onBlur={(event) => onPatch({ category: event.target.value })} />
        </div>
        <p className="evidence-claim"><span>原件位置</span>“{draft.evidenceText}”</p>
        {draft.baseAmountMinor === null ? (
          // 只提示不给入口是死路：用户只能丢弃草稿重解析整个来源（03 §3.5「三元组补全」）。
          <div className="triple-completion">
            <p className="triple-warning">确认前需要补全本位币与汇率</p>
            <input
              aria-label="本位币"
              maxLength={3}
              value={baseCurrency}
              placeholder="AUD"
              onChange={(event) => setBaseCurrency(event.target.value.toUpperCase())}
            />
            <input
              aria-label="汇率"
              value={rate}
              placeholder={`1 ${draft.currency} = ?`}
              onChange={(event) => setRate(event.target.value)}
            />
            <button
              disabled={baseCurrency.length !== 3 || parseRateInput(rate) === null}
              onClick={() => {
                const ratePpm = parseRateInput(rate)
                if (baseCurrency.length !== 3 || ratePpm === null) return
                onPatch({ baseCurrency, ratePpm })
              }}
            >补齐</button>
          </div>
        ) : (
          <p className="triple-rate">
            1 {draft.currency} = {formatRate(draft.ratePpm)} {draft.baseCurrency}
          </p>
        )}
      </div>
      <div className="draft-card__actions">
        <button onClick={onConfirm}>单条确认</button>
        <button className="danger" onClick={onDiscard}>丢弃</button>
      </div>
    </article>
  )
}
