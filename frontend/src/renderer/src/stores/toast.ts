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

export const createToastSlice: StateCreator<ToastSlice> = (set) => ({
  toasts: [],
  addToast: (toast) => {
    const id = crypto.randomUUID()
    const t: Toast = { ...toast, id }
    set((s: any) => {
      const next = [...s.toasts, t]
      if (next.length > MAX_TOASTS) next.splice(0, next.length - MAX_TOASTS)
      return { toasts: next }
    })
    // 自动消失计时器由 ToastItem 组件管理（支持悬停暂停 + 退场动画），
    // store 不再自行 setTimeout。
  },
  removeToast: (id) => {
    set((s: any) => ({ toasts: s.toasts.filter((t: Toast) => t.id !== id) }))
  },
})
