interface CompletedWithGapsBannerProps {
  outcome: string | null
  note: string | null
}

export function CompletedWithGapsBanner({ outcome, note }: CompletedWithGapsBannerProps) {
  if (outcome !== 'completed_with_gaps') return null
  return (
    <aside className="gap-banner" role="alert">
      <span className="gap-banner__mark" aria-hidden="true">!</span>
      <div>
        <strong>这份来源有未读区域</strong>
        <p>{note || 'Agent 标记了未完成内容，请对照原件逐条检查。'}</p>
      </div>
    </aside>
  )
}
