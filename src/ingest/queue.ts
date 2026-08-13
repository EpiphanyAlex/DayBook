export interface QueueFailure<T> {
  item: T
  error: unknown
}

export interface QueueResult<T, R> {
  completed: R[]
  failures: QueueFailure<T>[]
}

export async function runQueueContinuing<T, R>(
  items: readonly T[],
  operation: (item: T) => Promise<R>,
): Promise<QueueResult<T, R>> {
  const completed: R[] = []
  const failures: QueueFailure<T>[] = []
  for (const item of items) {
    try {
      completed.push(await operation(item))
    } catch (error) {
      failures.push({ item, error })
    }
  }
  return { completed, failures }
}
