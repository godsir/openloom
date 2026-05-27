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

export const createToastSlice: StateCreator<ToastSlice> = (set, get) => ({
  toasts: [],
  addToast: (toast) => {
    const id = crypto.randomUUID()
    const t: Toast = { ...toast, id }
    set((s: any) => ({ toasts: [...s.toasts, t] }))
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
