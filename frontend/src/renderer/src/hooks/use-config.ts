import { useState, useEffect, useCallback, useRef } from 'react'

interface ConfigState<T> {
  data: T | null
  loading: boolean
  error: string | null
  refresh: () => Promise<void>
}

const cache = new Map<string, { data: unknown; ts: number }>()
const STALE_MS = 5000

export function useConfig<T>(
  key: string,
  fetchFn: () => Promise<T>,
  fallback: T,
): ConfigState<T> {
  const [data, setData] = useState<T | null>(() => {
    const cached = cache.get(key)
    if (cached) return cached.data as T
    return null
  })
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const mounted = useRef(true)

  const refresh = useCallback(async () => {
    const cached = cache.get(key)
    if (cached && Date.now() - cached.ts < STALE_MS) {
      setData(cached.data as T)
      return
    }

    setLoading(true)
    try {
      const result = await fetchFn()
      if (mounted.current) {
        setData(result)
        setError(null)
        cache.set(key, { data: result, ts: Date.now() })
      }
    } catch (e: any) {
      if (mounted.current) setError(e.message)
    } finally {
      if (mounted.current) setLoading(false)
    }
  }, [key, fetchFn])

  useEffect(() => {
    refresh()
    return () => { mounted.current = false }
  }, [refresh])

  return { data, loading, error, refresh }
}
