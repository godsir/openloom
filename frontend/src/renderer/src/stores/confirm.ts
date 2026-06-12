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

export interface CronDetectedState {
  open: boolean
  name: string
  prompt: string
  cronExpression: string
  kind: string
  confirmation: string
  resolve: ((v: boolean) => void) | null
}

export interface ConfirmSlice {
  confirm: ConfirmState
  permissionConfirm: PermissionConfirmState
  cronDetected: CronDetectedState
  showConfirm: (title: string, message: string, danger?: boolean) => Promise<boolean>
  showPermissionConfirm: (title: string, message: string, toolName: string, danger?: boolean) => Promise<PermissionChoice>
  showCronDetected: (name: string, prompt: string, cronExpression: string, kind: string, confirmation: string) => Promise<boolean>
  setCronDetectedClosed: () => void
}

export const createConfirmSlice: StateCreator<ConfirmSlice> = (set, get) => ({
  confirm: { open: false, title: '', message: '', danger: false, resolve: null },
  permissionConfirm: { open: false, title: '', message: '', danger: false, toolName: '', resolve: null },
  cronDetected: { open: false, name: '', prompt: '', cronExpression: '', kind: '', confirmation: '', resolve: null },

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

  showCronDetected: (name: string, prompt: string, cronExpression: string, kind: string, confirmation: string) => {
    return new Promise<boolean>((resolve) => {
      // Release any previous hanging promise
      const prev = get().cronDetected
      if (prev.open && prev.resolve) {
        prev.resolve(false)
      }
      set({
        cronDetected: { open: true, name, prompt, cronExpression, kind, confirmation, resolve },
      })
    })
  },

  setCronDetectedClosed: () => {
    set({
      cronDetected: { open: false, name: '', prompt: '', cronExpression: '', kind: '', confirmation: '', resolve: null },
    })
  },
})
