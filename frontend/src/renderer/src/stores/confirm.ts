import { StateCreator } from 'zustand'

export interface ConfirmState {
  open: boolean
  title: string
  message: string
  danger: boolean
  resolve: ((v: boolean) => void) | null
}

export interface ConfirmSlice {
  confirm: ConfirmState
  showConfirm: (title: string, message: string, danger?: boolean) => Promise<boolean>
}

export const createConfirmSlice: StateCreator<ConfirmSlice> = (set, get) => ({
  confirm: { open: false, title: '', message: '', danger: false, resolve: null },

  showConfirm: (title: string, message: string, danger?: boolean) => {
    return new Promise<boolean>((resolve) => {
      set({
        confirm: { open: true, title, message, danger: !!danger, resolve },
      })
    })
  },
})
