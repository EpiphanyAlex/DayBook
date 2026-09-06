import { useState } from 'react'
import { verifiedSpan } from './evidenceSpan'
import type { EvidenceContent, ReviewDraft } from './types'

interface Props {
  sourceId: string
  content: EvidenceContent | null
  draft: ReviewDraft | null
  error: Error | null
  onRetry: () => void
  onImageState: (ready: boolean) => void
}
export function EvidencePane({ sourceId, content, draft, error, onRetry, onImageState }: Props) {
  const [decodeFailed, setDecodeFailed] = useState(false)
  const span = content?.kind === 'utterance' && content.text !== null && draft ? verifiedSpan(content.text, draft) : null
  return <div className="evidence-pane" data-source-id={sourceId}>
    <div className="light-table">
      {error ? <div role="alert"><p>原件读取失败：{error.message}</p><button onClick={onRetry}>重读原件</button></div>
        : !content ? <p className="stage-loading">正在取出原件…</p>
        : content.kind === 'utterance' && content.text !== null ? <pre data-testid="original-text">{span ? <>{span[0]}<mark>{span[1]}</mark>{span[2]}</> : content.text}</pre>
        : content.dataBase64 ? <>
          {!decodeFailed && <img src={`data:${content.mimeType};base64,${content.dataBase64}`} alt="导入的来源原图"
            onLoad={() => onImageState(true)} onError={() => { setDecodeFailed(true); onImageState(false) }} />}
          {decodeFailed && <div role="alert"><p>图片无法解码，暂不能确认。</p><button onClick={onRetry}>重读原件</button></div>}
        </> : <div role="alert"><p>原件内容不可用，暂不能确认。</p><button onClick={onRetry}>重读原件</button></div>}
    </div>
    {draft && <aside className="current-claim"><strong>当前草稿 · 抽取声明</strong><blockquote>{draft.evidenceText}</blockquote>
      {content?.kind === 'utterance' && !span && <p>无法定位这条声明，请对照完整原文核对。</p>}
    </aside>}
  </div>
}
