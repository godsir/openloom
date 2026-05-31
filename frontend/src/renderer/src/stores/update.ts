import { StateCreator } from 'zustand'

export interface UpdateState {
  status: 'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'no-update' | 'error'
  version: string | null
  progress: number
  bytesPerSecond: number
  transferred: number
  total: number
  error: string | null
}

const initialUpdate: UpdateState = {
  status: 'idle',
  version: null,
  progress: 0,
  bytesPerSecond: 0,
  transferred: 0,
  total: 0,
  error: null,
}

export interface UpdateSlice {
  update: UpdateState
  updateModalOpen: boolean
  dismissedVersion: string | null

  onAutoUpdateAvailable: (version: string | null) => void
  onAutoUpdateNotAvailable: () => void
  onAutoDownloadProgress: (progress: { percent: number; bytesPerSecond: number; transferred: number; total: number }) => void
  onAutoUpdateDownloaded: () => void
  onAutoUpdateError: (error: string) => void

  checkUpdate: () => Promise<void>
  downloadUpdate: () => Promise<void>
  installUpdate: () => void
  dismissUpdate: () => void
  simulateUpdateFlow: () => void
}

let _simulateTimer: ReturnType<typeof setInterval> | null = null

function clearSimulateTimer() {
  if (_simulateTimer) {
    clearInterval(_simulateTimer)
    _simulateTimer = null
  }
}

let _isSimulating = false

export const createUpdateSlice: StateCreator<UpdateSlice> = (set, get) => ({
  update: { ...initialUpdate },
  updateModalOpen: false,
  dismissedVersion: null,

  onAutoUpdateAvailable: (version: string | null) => {
    const dismissed = get().dismissedVersion
    set({
      update: { ...initialUpdate, status: 'available', version },
      updateModalOpen: version !== dismissed,
    })
  },

  onAutoUpdateNotAvailable: () => {
    set({ update: { ...initialUpdate, status: 'no-update' } })
  },

  onAutoDownloadProgress: (progress) => {
    set({
      update: {
        ...get().update,
        status: 'downloading',
        progress: progress.percent,
        bytesPerSecond: progress.bytesPerSecond,
        transferred: progress.transferred,
        total: progress.total,
      },
    })
  },

  onAutoUpdateDownloaded: () => {
    set({
      update: { ...get().update, status: 'downloaded', progress: 100 },
    })
  },

  onAutoUpdateError: (error: string) => {
    set({
      update: { ...get().update, status: 'error', error },
    })
  },

  checkUpdate: async () => {
    set({ update: { ...initialUpdate, status: 'checking' } })
    try {
      await window.loom.checkForUpdates()
    } catch {
      set({ update: { ...get().update, status: 'error', error: '检查更新失败' } })
    }
  },

  downloadUpdate: async () => {
    set({ update: { ...get().update, status: 'downloading', progress: 0 } })

    if (_isSimulating) {
      const totalBytes = 118_651_946
      clearSimulateTimer()
      let progress = 0
      _simulateTimer = setInterval(() => {
        progress += Math.random() * 8 + 2
        if (progress >= 100) {
          clearSimulateTimer()
          set({
            update: { ...get().update, status: 'downloaded', progress: 100, transferred: totalBytes, bytesPerSecond: 0 },
          })
          _isSimulating = false
          return
        }
        set({
          update: {
            ...get().update,
            progress,
            transferred: Math.floor(totalBytes * progress / 100),
            total: totalBytes,
            bytesPerSecond: Math.floor(Math.random() * 2_000_000 + 500_000),
          },
        })
      }, 200)
      return
    }

    try {
      await window.loom.downloadUpdate()
    } catch {
      set({ update: { ...get().update, status: 'error', error: '下载失败' } })
    }
  },

  installUpdate: () => {
    if (_isSimulating) {
      _isSimulating = false
      set({ update: { ...initialUpdate }, updateModalOpen: false })
      return
    }
    window.loom.installUpdate()
  },

  dismissUpdate: () => {
    clearSimulateTimer()
    if (_isSimulating) {
      _isSimulating = false
      set({ update: { ...initialUpdate }, updateModalOpen: false })
      return
    }
    const version = get().update.version
    set({ updateModalOpen: false, dismissedVersion: version })
  },

  simulateUpdateFlow: () => {
    clearSimulateTimer()
    _isSimulating = true
    set({
      update: { ...initialUpdate, status: 'available', version: '9.9.9-test' },
      updateModalOpen: true,
      dismissedVersion: null,
    })
  },
})
