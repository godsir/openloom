import { create } from 'zustand'
import { StateCreator } from 'zustand'

export type ToastType = 'info' | 'success' | 'warning' | 'error'

export interface Toast {
  id: string
  type: ToastType
  message: string
  duration?: number
  action?: { label: string; onClick: () => void }
}

export interface ToastSlice {
  toasts: Toast[]
  addToast: (toast: Omit<Toast, 'id'>) => void
  removeToast: (id: string) => void
}

const MAX_TOASTS = 8

export const createToastSlice: StateCreator<ToastSlice> = (set, get) => ({
  toasts: [],
  addToast: (toast) => {
    const id = crypto.randomUUID()
    const t: Toast = { ...toast, id }
    set((s: any) => {
      const next = [...s.toasts, t]
      if (next.length > MAX_TOASTS) next.splice(0, next.length - MAX_TOASTS)
      return { toasts: next }
    })
    const duration = toast.duration ?? 4000
    if (duration > 0) {
      setTimeout(() => {
        get().removeToast(id)
      }, duration)
    }
  },
  removeToast: (id) => {
    set((s: any) => ({ toasts: s.toasts.filter((t: Toast) => t.id !== id) }))
  },
})
