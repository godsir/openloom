import { StateCreator } from 'zustand'

export type KeyedSlice<T> = {
  data: Map<string, T>
  get: (key: string) => T | undefined
  set: (key: string, value: T) => void
  delete: (key: string) => void
  clear: () => void
}

export function createKeyedSlice<T>(
  set: (fn: (state: any) => any) => void,
  get: () => any,
  storeKey: string,
): KeyedSlice<T> {
  return {
    data: new Map(),
    get: (key: string) => get()[storeKey].data.get(key),
    set: (key: string, value: T) =>
      set((s: any) => {
        const next = new Map(s[storeKey].data)
        next.set(key, value)
        return { [storeKey]: { ...s[storeKey], data: next } }
      }),
    delete: (key: string) =>
      set((s: any) => {
        const next = new Map(s[storeKey].data)
        next.delete(key)
        return { [storeKey]: { ...s[storeKey], data: next } }
      }),
    clear: () =>
      set((s: any) => ({
        [storeKey]: { ...s[storeKey], data: new Map() },
      })),
  }
}
