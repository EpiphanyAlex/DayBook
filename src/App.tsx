import { useQuery, useQueryClient } from '@tanstack/react-query'
import { EvidencePane } from './review/EvidencePane'
import { verifiedSpan } from './review/evidenceSpan'
import { DraftCard } from './review/DraftCard'
import { ReconciliationCard } from './review/ReconciliationCard'
import { initialScreen, screenReducer, attemptKey } from './review/screenReducer'
import { useReviewMutation } from './review/mutations'
import { keys, sourcesOptions, agentOptions, foundationOptions, logsOptions, useReviewQueries, refreshSources as reloadSources, refreshReview, probeOnce } from './review/queries'
import type { ReviewSource, ReviewDraft, FoundationStatus, ImportResult } from './review/types'
import { useCallback, useEffect, useReducer, useRef, useState } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import iconUrl from '../assets/brand/icon.svg'
import { backendPresentation } from './agent/presentation'
import { runQueueContinuing } from './ingest/queue'
import { AppError, call } from './lib/bridge'
import { formatMoney, parseMoneyInput, parseRateInput } from './lib/money'
import { AttestationHint } from './review/AttestationHint'
import { CompletedWithGapsBanner } from './review/CompletedWithGapsBanner'
import {
  canBatchConfirm,
  POLICY,
  RECONCILIATION,
  SOURCE_STATE,
  type SourceStateValue,
  type UtteranceGateState,
} from './review/policy'

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
  const client = useQueryClient()
  const [ui, dispatch] = useReducer(screenReducer, initialScreen)
  const uiRef = useRef(ui)
  uiRef.current = ui
  const [baseCurrencyInput, setBaseCurrencyInput] = useState<string | null>(null)
  const foundationQuery = useQuery({ ...foundationOptions, enabled: isTauri() })
  const backendQuery = useQuery({ ...agentOptions, enabled: isTauri() })
  const sourcesQuery = useQuery({ ...sourcesOptions, enabled: isTauri() })
  const logsQuery = useQuery({ ...logsOptions, enabled: isTauri() })
  const foundation = foundationQuery.data ?? null
  const backend = backendQuery.data ?? null
  const sources = sourcesQuery.data ?? []
  const agentLogs = logsQuery.data ?? []
  const selectedId = ui.selectedId ?? sources[0]?.id ?? null
  const selectedSource = sources.find(source => source.id === selectedId) ?? null
  const { drafts, evidence, check: reviewCheck, ready, error: reviewError, evidenceQuery } = useReviewQueries(selectedSource)
  const check = selectedSource?.state === SOURCE_STATE.PARSING ? null : reviewCheck
  const scope = attemptKey(selectedId ?? '', selectedSource?.latestAttemptId ?? null)
  const excluded = ui.excludedByAttempt[scope] ?? []
  const selectedList = drafts.filter(draft => !excluded.includes(draft.id))
  const selectedDrafts = new Set(selectedList.map(draft => draft.id))
  const focusedDraft = drafts.find(d => d.id === ui.focusedByAttempt[scope]) ?? drafts[0] ?? null
  const [imageReady, setImageReady] = useState<string | null>(null)
  const imageIdentity = `${selectedId}:${evidenceQuery.dataUpdatedAt}`
  const imageIdentityRef = useRef(imageIdentity)
  imageIdentityRef.current = imageIdentity
  const originalReady = Boolean(evidence && (evidence.kind === 'utterance' ? typeof evidence.text === 'string' && evidence.text.length > 0 : imageReady === imageIdentity))
  const mutation = useReviewMutation()
  const [utterance, setUtterance] = useState('')
  const utteranceRevision = useRef(0)
  const [notice, setNotice] = useState<string | null>(null)
  const [readFailure, setReadFailure] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const busyLock = useRef(false)
  const stopLock = useRef(false)
  const [stopping, setStopping] = useState(false)
  const submitToken = useRef<string | null>(null)
  const canReview = ready && originalReady && !sourcesQuery.isError && !sourcesQuery.isFetching && !mutation.pending && !busy && selectedSource?.state !== SOURCE_STATE.PARSING
  const editKey = (draft: ReviewDraft) => `${attemptKey(draft.sourceId, draft.attemptId)}:${draft.id}`
  const hasUnsaved = drafts.some(d => Object.keys(ui.edits[editKey(d)]?.values ?? {}).length > 0)

  const refreshSources = useCallback(async (preferId?: string, revision?: number) => {
    const next = await reloadSources(client)
    if (preferId && revision === uiRef.current.selectionRevision) dispatch({ type: 'select', id: preferId })
    return next
  }, [client])
  const refreshAgentLogs = useCallback(async () => {
    await client.invalidateQueries({ queryKey: keys.logs })
  }, [client])
  useEffect(() => {
    if (isTauri()) void probeOnce(client).catch(error => setNotice(errorMessage(error)))
  }, [client])

  const parseImported = useCallback(async (result: ImportResult, selectionRevision: number) => {
    if (result.deduplicated) {
      setNotice('这份来源已经导入过，已为你定位到原记录。')
      await refreshSources(result.sourceId, selectionRevision)
      return
    }
    await refreshSources(result.sourceId, selectionRevision)
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
      await Promise.all([refreshReview(client, result.sourceId), refreshAgentLogs()])
    }
  }, [backend, client, refreshAgentLogs, refreshSources])

  const importPaths = useCallback(async (paths: string[]) => {
    if (!paths.length || busyLock.current) return
    busyLock.current = true
    const selectionRevision = uiRef.current.selectionRevision
    setBusy(true)
    const result = await runQueueContinuing(paths, async (path) => {
        const result = await call<ImportResult>('import_source_file', { path })
        await parseImported(result, selectionRevision)
        return result.sourceId
    })
    if (result.failures.length) {
      const first = result.failures[0]
      setNotice(`${result.failures.length} 份来源未完成（${first ? errorMessage(first.error) : '未知错误'}）；其余来源已继续处理。`)
    }
    setBusy(false)
    busyLock.current = false
  }, [parseImported])

  useEffect(() => {
    if (!isTauri()) return
    let unlisten: (() => void) | undefined
    let disposed = false
    void getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'drop') void importPaths(event.payload.paths)
    }).then((dispose) => { if (disposed) dispose(); else unlisten = dispose }).catch(error => setNotice(errorMessage(error)))
    return () => { disposed = true; unlisten?.() }
  }, [importPaths])

  const submitUtterance = async () => {
    if (!utterance.trim() || busyLock.current) return
    busyLock.current = true
    const selectionRevision = uiRef.current.selectionRevision
    const inputRevision = utteranceRevision.current
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
      if (inputRevision === utteranceRevision.current) setUtterance('')
      await parseImported(result, selectionRevision)
    } catch (error) {
      setNotice(errorMessage(error))
    } finally {
      setBusy(false)
      busyLock.current = false
    }
  }

  const saveBaseCurrency = async () => {
    const currency = (baseCurrencyInput ?? foundation?.baseCurrency ?? '').trim().toUpperCase()
    if (!currency) return
    try {
      const status = await call<FoundationStatus>('set_base_currency', { currency })
      client.setQueryData(keys.foundation, status)
      setBaseCurrencyInput(current => current === baseCurrencyInput ? null : current)
      setNotice(`之后的新解析将使用 ${currency} 作为本位币；已有记录不会改变。`)
    } catch (error) {
      setNotice(errorMessage(error))
    }
  }

  const toggleDebugLogging = async () => {
    if (!foundation) return
    try {
      const status = await call<FoundationStatus>('set_debug_logging', { enabled: !foundation.debugLogging })
      client.setQueryData(keys.foundation, status)
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
    if (busyLock.current) return
    busyLock.current = true
    setBusy(true)
    try {
      await call('parse_source', { sourceId })
      setNotice('重新解析完成。')
    } catch (error) {
      setNotice(errorMessage(error))
    } finally {
      try { await Promise.all([refreshReview(client, sourceId), refreshAgentLogs()]) }
      catch (error) { setNotice(errorMessage(error)) }
      setBusy(false)
      busyLock.current = false
    }
  }

  const stopParsing = async () => {
    if (stopLock.current) return
    stopLock.current = true
    setStopping(true)
    try {
      const stopped = await call<boolean>('cancel_parsing')
      setNotice(stopped ? '停止请求已完成，正在读取最新状态。' : '当前没有正在运行的解析。')
    } catch (error) { setNotice(errorMessage(error)) }
    finally {
      try { await Promise.all([selectedId ? refreshReview(client, selectedId) : refreshSources(), refreshAgentLogs()]) }
      catch (error) { setNotice(errorMessage(error)) }
      stopLock.current = false
      setStopping(false)
    }
  }

  const gateState: UtteranceGateState = {
    fullSourceVisible: originalReady && selectedSource?.kind === 'utterance',
    resultsAdjacent: ready && drafts.length > 0 && drafts.every(d => evidence?.text !== null && evidence?.text !== undefined && verifiedSpan(evidence.text, d) !== null),
    itemCountVisible: ready && drafts.length > 0,
  }
  const batchReady = Boolean(canReview && !hasUnsaved && check && selectedList.length && canBatchConfirm(check, gateState))
  const write = async (command: string, args: Record<string, unknown>, message: string) => {
    if (!canReview || !selectedSource?.latestAttemptId) return null
    const sourceId = selectedSource.id
    try {
      const outcome = await mutation.execute({ command, args, sourceId, attemptId: selectedSource.latestAttemptId })
      if (!outcome) return null
      setNotice(message)
      if (outcome.readFailed) setReadFailure(sourceId)
      return outcome
    } catch (error) { setNotice(errorMessage(error)); throw error }
  }
  const confirmOne = async (draftId: string) => {
    if (hasUnsaved || !drafts.some(d => d.id === draftId)) return
    try { await write('confirm_draft', { draftId }, '已确认 1 条交易。') } catch { /* 保留草稿与错误 */ }
  }
  const discard = async (draftId: string) => {
    if (!drafts.some(d => d.id === draftId)) return
    try { await write('discard_draft', { draftId }, '这条草稿已丢弃，原始记录仍保留在审计中。') } catch { /* 保留草稿与错误 */ }
  }
  const confirmSelected = async () => {
    if (!batchReady || !check) return
    try {
      const outcome = await write('confirm_drafts', {
        draftIds: selectedList.map(d => d.id),
        attestation: check.confirmationPolicy === POLICY.USER_ATTESTED_BATCH ? gateState : null,
      }, '确认操作已完成。')
      if (outcome) {
        const result = outcome.result as { confirmed: unknown[]; rejected: { message: string }[] }
        setNotice(`已确认 ${result.confirmed.length} 条交易。` + (result.rejected.length ? ` ${result.rejected.length} 条未确认：${result.rejected.map(r => r.message).join('；')}` : ''))
      }
    } catch { /* 不重发写命令 */ }
  }
  const saveDraft = async (draft: ReviewDraft, extra?: Record<string, string>) => {
    const key = editKey(draft)
    const edit = uiRef.current.edits[key]
    const values = { ...edit?.values, ...extra }
    if (!Object.keys(values).length || !canReview) return
    const revision = edit?.revision ?? 0
    const patch: Record<string, unknown> = {}
    try {
      for (const [field, value] of Object.entries(values)) {
        if (field === 'amount') {
          const minor = parseMoneyInput(value, draft.currency)
          if (minor === null || Math.abs(minor) > 1e15) throw new Error('金额只能是支持精度与范围内的数字')
          patch.amountMinor = minor
        } else if (field === 'rate' || field === 'baseCurrency') {
          const currency = values.baseCurrency ?? foundation?.baseCurrency ?? ''
          const rate = parseRateInput(values.rate ?? (draft.currency === currency ? '1' : ''))
          if (currency.length !== 3 || rate === null) throw new Error('请填写三位本位币代码与有效汇率')
          patch.baseCurrency = currency
          patch.ratePpm = rate
        } else patch[field] = value
      }
      const outcome = await write('update_draft', { draftId: draft.id, patch }, '草稿修改已保存。')
      if (outcome) dispatch({ type: 'saved', key, revision, fields: Object.keys(values) })
    } catch (error) {
      setNotice(error instanceof Error ? error.message : errorMessage(error))
      dispatch({ type: 'edit-error', key, revision, error: error instanceof Error ? error.message : errorMessage(error) })
    }
  }
  const retryRead = async () => {
    if (!readFailure) return
    try { await refreshReview(client, readFailure); setReadFailure(null) }
    catch (error) { setNotice(errorMessage(error)) }
  }

  const agentView = backendPresentation(backend)

  const baseCurrencyForm = (<form className="base-currency-setting" onSubmit={(event) => { event.preventDefault(); void saveBaseCurrency() }}>
            <label htmlFor="base-currency">当前本位币</label>
            <div>
              <input
                id="base-currency"
                list="currency-suggestions"
                maxLength={3}
                className="field-boxed"
                value={baseCurrencyInput ?? foundation?.baseCurrency ?? ''}
                placeholder="例如 AUD"
                onChange={(event) => setBaseCurrencyInput(event.target.value.toUpperCase())}
              />
              <datalist id="currency-suggestions">
                {['AUD', 'CNY', 'USD', 'EUR', 'GBP', 'JPY', 'HKD', 'NZD', 'SGD', 'CAD'].map((currency) => <option key={currency} value={currency} />)}
              </datalist>
              <button type="submit" disabled={(baseCurrencyInput ?? foundation?.baseCurrency ?? '').length !== 3}>设定</button>
            </div>
            {!foundation?.baseCurrency && <small>首次解析前请明确选择；不会按地区猜测。</small>}
          </form>)

  const composerForm = (<form className="utterance-box composer" onSubmit={(event) => { event.preventDefault(); void submitUtterance() }}>
            <label htmlFor="utterance">或者，说一段过去的事</label>
            <textarea
              id="utterance"
              value={utterance}
              onChange={(event) => { utteranceRevision.current++; setUtterance(event.target.value) }}
              placeholder="昨天午饭 18，回家打车 24…"
              rows={4}
            />
            <button type="submit" disabled={!utterance.trim() || busy}>保存并拆分</button>
          </form>)

  return (
    <main className={`workbench ${sources.length ? '' : 'is-empty'}`} data-theme="cloud" data-region="paper">
      <header className="topbar" data-region="rail">
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
        <aside data-region="rail" className="source-drawer" aria-label="来源">
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

          {sources.length > 0 && baseCurrencyForm}

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

          {sources.length > 0 && composerForm}

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
                  onClick={() => dispatch({ type: 'select', id: source.id })}
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
                    disabled={stopping}
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
              <EvidencePane key={imageIdentity} sourceId={selectedSource.id} content={evidence} draft={focusedDraft}
                error={evidenceQuery.error} onRetry={() => { setImageReady(null); void evidenceQuery.refetch() }}
                onImageState={value => { if (imageIdentityRef.current === imageIdentity) setImageReady(value ? imageIdentity : null) }} />
              {foundation && <p className="data-footnote">本地证据库 · schema {foundation.schemaVersion}</p>}
            </>
          ) : (
            <div className="empty-evidence">
              <img src={iconUrl} alt="" />
              <p>你的个人事务助理</p>
              <h1>把零散的钱和事，整理清楚。</h1>
              <span>拖入截图，或说一段话。不用逐条填表。</span>
              {composerForm}
              {baseCurrencyForm}
              <aside className="privacy-card"><strong>你的数据如何保存和解析</strong><p>账本、原件与日志保存在这台 Mac。截图和文字由你选择的 CLI 发往其模型服务商，使用你的额度。</p><p>解析先生成草稿，经你确认后才入账。Daybook 无账号、无远程服务端、不收集遥测。</p></aside>
            </div>
          )}
        </section>

        <section className="review-tray" aria-label="待确认草稿">
          <header className="review-heading">
            <div><p>拆分结果</p><h2>{drafts.length} 条待确认</h2></div>
            {drafts.length > 0 && (
              <button className="select-all" onClick={() => dispatch({ type: 'exclude', scope, ids: selectedDrafts.size === drafts.length ? drafts.map(d => d.id) : [] })}>{selectedDrafts.size === drafts.length ? '取消全选' : '全选'}</button>
            )}
          </header>

          <CompletedWithGapsBanner outcome={check?.outcome ?? null} note={check?.unparsedNote ?? null} />


          <div className="draft-stack">
            {drafts.map((draft) => (
              <DraftCard
                key={draft.id}
                draft={draft}
                selected={selectedDrafts.has(draft.id)}
                unavailable={check?.unavailableDraftIds.includes(draft.id) ?? false}
                onToggle={() => dispatch({ type: 'exclude', scope, ids: excluded.includes(draft.id) ? excluded.filter(id => id !== draft.id) : [...excluded, draft.id] })}
                onFocus={() => dispatch({ type: 'focus', scope, id: draft.id })}
                disabled={!canReview || hasUnsaved && !Object.keys(ui.edits[editKey(draft)]?.values ?? {}).length}
                edit={ui.edits[editKey(draft)]}
                onEdit={(field, value) => dispatch({ type: 'edit', key: editKey(draft), field, value })}
                onSave={(extra) => void saveDraft(draft, extra)}
                onRestore={() => dispatch({ type: 'restore', key: editKey(draft) })}
                onConfirm={() => void confirmOne(draft.id)}
                onDiscard={() => void discard(draft.id)}
                defaultBaseCurrency={foundation?.baseCurrency ?? ''}
              />
            ))}
            {selectedSource && !drafts.length && (
              <div className={`empty-drafts ${selectedSource.state === SOURCE_STATE.PARSING ? 'is-parsing' : ''}`}>
                <p>{selectedSource.state === SOURCE_STATE.PARSING ? '正在辨认原件里的交易…' : '这里还没有待确认条目。'}</p>
                {selectedSource.parseErrorCode && <small>{selectedSource.parseErrorCode}</small>}
              </div>
            )}
          </div>

          <footer className="review-actions">
            {check && <ReconciliationCard check={check} />}
            {!originalReady && selectedSource && <p>原件尚未就绪，确认已暂停。</p>}
            {hasUnsaved && <p>有未保存修改，请先保存或恢复。</p>}
            {reviewError && <div role="alert"><p>审核数据读取失败：{reviewError.message}</p><button onClick={() => selectedId && void refreshReview(client, selectedId).catch(error => setNotice(errorMessage(error)))}>重读审核数据</button></div>}
            {readFailure && <div role="alert"><p>操作已完成，读取失败。</p><button onClick={() => void retryRead()}>重试读取</button></div>}

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
      {sourcesQuery.error && <div role="alert">来源读取失败：{sourcesQuery.error.message}<button onClick={() => void refreshSources().catch(error => setNotice(errorMessage(error)))}>重读来源</button></div>}
      {notice && <button className="notice" onClick={() => setNotice(null)}>{notice}<span>关闭</span></button>}
    </main>
  )
}
