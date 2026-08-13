import { describe, expect, it, vi } from 'vitest'
import { runQueueContinuing } from './queue'

describe('ingest/batch-continues', () => {
  it('isolates one source failure and continues the remaining queue', async () => {
    const operation = vi.fn(async (source: string) => {
      if (source === 'broken.png') throw new Error('unsupported')
      return `${source}:parsed`
    })

    const result = await runQueueContinuing(
      ['first.png', 'broken.png', 'last.png'],
      operation,
    )

    expect(operation.mock.calls.map(([source]) => source)).toEqual([
      'first.png',
      'broken.png',
      'last.png',
    ])
    expect(result.completed).toEqual(['first.png:parsed', 'last.png:parsed'])
    expect(result.failures).toHaveLength(1)
    expect(result.failures[0]?.item).toBe('broken.png')
  })
})
