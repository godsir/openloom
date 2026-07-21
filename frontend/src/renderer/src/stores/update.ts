import { StateCreator } from 'zustand'
import { t } from '../i18n'

export interface UpdateState {
  status: 'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'no-update' | 'error'
  version: string | null
  releaseNotes: string | null
  progress: number
  bytesPerSecond: number
  transferred: number
  total: number
  error: string | null
}

const initialUpdate: UpdateState = {
  status: 'idle',
  version: null,
  releaseNotes: null,
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

  onAutoUpdateAvailable: (version: string | null, releaseNotes?: string | null) => void
  onAutoUpdateNotAvailable: () => void
  onAutoDownloadProgress: (progress: { percent: number; bytesPerSecond: number; transferred: number; total: number }) => void
  onAutoUpdateDownloaded: () => void
  onAutoDownloadCancelled: () => void
  onAutoUpdateError: (error: string) => void

  checkUpdate: () => Promise<void>
  downloadUpdate: () => Promise<void>
  cancelDownload: () => void
  installUpdate: () => void
  dismissUpdate: () => void
  closeUpdateModal: () => void
  backgroundDownload: () => Promise<void>
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

// 检查更新兜底计时器：electron-updater 在 checking 阶段出错时可能被静默吞掉
// （resolve(null) 且不 emit 任何事件），没有兜底会让状态永远卡在 'checking'。
let _checkTimer: ReturnType<typeof setTimeout> | null = null
function clearCheckTimer() {
  if (_checkTimer) {
    clearTimeout(_checkTimer)
    _checkTimer = null
  }
}

export const createUpdateSlice: StateCreator<UpdateSlice> = (set, get) => ({
  update: { ...initialUpdate },
  updateModalOpen: false,
  dismissedVersion: null,

  onAutoUpdateAvailable: (version: string | null, releaseNotes?: string | null) => {
    clearCheckTimer()
    const dismissed = get().dismissedVersion
    // Clear __error__ dismissal when a new version is found
    set({
      update: { ...initialUpdate, status: 'available', version, releaseNotes: releaseNotes ?? null },
      updateModalOpen: version !== dismissed,
      dismissedVersion: dismissed === '__error__' ? null : dismissed,
    })
  },

  onAutoUpdateNotAvailable: () => {
    clearCheckTimer()
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
      updateModalOpen: true,
    })
  },

  onAutoDownloadCancelled: () => {
    // 取消后回到"有可用更新"态，进度清零，允许用户重新下载（C20）
    set({
      update: { ...get().update, status: 'available', progress: 0, bytesPerSecond: 0, transferred: 0, total: 0 },
    })
  },

  onAutoUpdateError: (error: string) => {
    clearCheckTimer()
    const state = get()
    // If user dismissed this error, don't re-display the island
    if (state.dismissedVersion === '__error__') return
    set({
      update: { ...state.update, status: 'error', error },
    })
  },

  checkUpdate: async () => {
    clearCheckTimer()
    set({ update: { ...initialUpdate, status: 'checking' } })
    // 25s 兜底：checking 阶段若迟迟没有结果（事件被吞/resolve(null)），主动
    // 落到 no-update，避免永久卡在 'checking' 且"检查更新"按钮消失。
    _checkTimer = setTimeout(() => {
      _checkTimer = null
      if (get().update.status === 'checking') {
        set({ update: { ...initialUpdate, status: 'no-update' } })
      }
    }, 25000)
    try {
      await window.loom.checkForUpdates()
    } catch {
      // A failed metadata check (for example, no network) is not a failed
      // update download. Leave the island idle and let the user retry later.
      clearCheckTimer()
      set({ update: { ...initialUpdate } })
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
      set({ update: { ...get().update, status: 'error', error: t('updates.downloadFailed') } })
    }
  },

  cancelDownload: () => {
    // 模拟流程：直接停掉进度定时器并回到可下载态
    if (_isSimulating) {
      clearSimulateTimer()
      _isSimulating = false
      set({ update: { ...get().update, status: 'available', progress: 0 } })
      return
    }
    // 真实下载：通知主进程取消（CancellationToken），主进程会回传 cancelled 事件
    window.loom.cancelDownloadUpdate()
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
    const state = get()
    // If dismissing an error, block future re-displays for this update cycle
    const v = state.update.status === 'error' ? '__error__' : state.update.version
    set({ update: { ...initialUpdate }, updateModalOpen: false, dismissedVersion: v })
  },

  closeUpdateModal: () => {
    set({ updateModalOpen: false })
  },

  backgroundDownload: async () => {
    get().downloadUpdate()
    set({ updateModalOpen: false })
    // 关闭弹窗后提示一次"已转后台下载"，把用户视线引向灵动岛进度
    ;(get() as any).showIslandTransient?.(t('updates.backgroundStarted'), 2500)
  },

  simulateUpdateFlow: () => {
    clearSimulateTimer()
    _isSimulating = true
    set({
      update: { ...initialUpdate, status: 'available', version: '9.9.9-test', releaseNotes: '## What\'s Changed\n\n- 新增后台下载功能\n- 修复更新提示文案\n- 性能优化与稳定性改进\n\n**Full Changelog**: https://github.com/godsir/openloom/compare/v0.2.18...v9.9.9-test' },
      updateModalOpen: true,
      dismissedVersion: null,
    })
  },
})
