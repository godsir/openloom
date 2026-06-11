import { StateCreator } from 'zustand'

export interface ConfirmState {
  open: boolean
  title: string
  message: string
  danger: boolean
  resolve: ((v: boolean) => void) | null
}

export type PermissionChoice = 'approve' | 'approve_always' | 'deny'

export interface PermissionConfirmState {
  open: boolean
  title: string
  message: string
  danger: boolean
  toolName: string
  resolve: ((v: PermissionChoice) => void) | null
}

export interface ConfirmSlice {
  confirm: ConfirmState
  permissionConfirm: PermissionConfirmState
  showConfirm: (title: string, message: string, danger?: boolean) => Promise<boolean>
  showPermissionConfirm: (title: string, message: string, toolName: string, danger?: boolean) => Promise<PermissionChoice>
}

export const createConfirmSlice: StateCreator<ConfirmSlice> = (set, get) => ({
  confirm: { open: false, title: '', message: '', danger: false, resolve: null },
  permissionConfirm: { open: false, title: '', message: '', danger: false, toolName: '', resolve: null },

  showConfirm: (title: string, message: string, danger?: boolean) => {
    return new Promise<boolean>((resolve) => {
      set({
        confirm: { open: true, title, message, danger: !!danger, resolve },
      })
    })
  },

  showPermissionConfirm: (title: string, message: string, toolName: string, danger?: boolean) => {
    return new Promise<PermissionChoice>((resolve) => {
      set({
        permissionConfirm: { open: true, title, message, danger: !!danger, toolName, resolve },
      })
    })
  },
})
