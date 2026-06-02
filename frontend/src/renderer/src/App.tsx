import { useEffect, useRef, useState } from 'react'
import AppShell from './components/app/AppShell'
import SettingsModal from './components/shared/SettingsModal'
import UpdateModal from './components/shared/UpdateModal'

import Onboarding from './components/shared/Onboarding'
import ErrorBoundary from './components/shared/ErrorBoundary'
import ToastContainer from './components/shared/ToastContainer'
import ConfirmDialog from './components/shared/ConfirmDialog'
import { bootstrapApp } from './services/bootstrap'
import { handleModelsChanged } from './services/app-event-actions'
import { useStore } from './stores'
import styles from './App.module.css'

export default function App() {
  const [ready, setReady] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [showOnboarding, setShowOnboarding] = useState(false)
  const retryCleanupRef = useRef<(() => void) | null>(null)
  const settingsOpen = useStore((s) => s.settingsOpen)
  const setSettingsOpen = useStore((s) => s.setSettingsOpen)
  const theme = useStore((s) => s.theme)
  const confirm = useStore((s) => s.confirm)
  const set = useStore.setState

  // Apply theme on mount and on change
  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme)
  }, [theme])

  useEffect(() => {
    let cancelled = false
    let teardown: (() => void) | null = null
    async function boot() {
      try {
        const cleanup = await bootstrapApp()
        if (cancelled) { cleanup(); return }
        teardown = cleanup
        setReady(true)
        const pref = await window.loom.getPreference('onboarded', false)
        if (!pref) setShowOnboarding(true)
        const savedTheme = await window.loom.getPreference('theme', 'dark')
        useStore.getState().setTheme(savedTheme as any)
        if (savedTheme === 'custom') {
          const cc = await window.loom.getPreference('customTheme', { bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' })
          // Re-apply custom theme colors on boot
          const root = document.documentElement
          const { bg, surface, text, accent } = cc as any
          const hexToRgb = (hex: string): [number, number, number] => {
            const v = parseInt(String(hex).replace('#', ''), 16)
            return [(v >> 16) & 255, (v >> 8) & 255, v & 255]
          }
          const [ar, ag, ab] = hexToRgb(accent)
          const isLight = bg > '#888'
          root.style.setProperty('--bg', bg)
          root.style.setProperty('--bg-surface', surface)
          root.style.setProperty('--bg-card', surface)
          root.style.setProperty('--text', text)
          root.style.setProperty('--accent', accent)
          root.style.setProperty('--accent-rgb', `${ar},${ag},${ab}`)
          root.style.setProperty('--accent-subtle', `rgba(${ar},${ag},${ab},0.10)`)
          root.style.setProperty('--accent-medium', `rgba(${ar},${ag},${ab},0.16)`)
          root.style.setProperty('--border-accent', `rgba(${ar},${ag},${ab},0.28)`)
        }
        const savedFontSize = await window.loom.getPreference('fontSize', 'default')
        useStore.getState().setFontSize(savedFontSize as any)
        const savedPinned = await window.loom.getPreference<string[]>('pinnedIds', [])
        if (savedPinned.length) {
          useStore.setState({ pinnedIds: new Set(savedPinned) })
        }
      } catch (e: any) {
        if (cancelled) return
        setError(e.message || '启动失败')
      }
    }
    boot()
    return () => { cancelled = true; teardown?.() }
  }, [])

  // Global update IPC listeners (registered once on mount)
  useEffect(() => {
    window.loom.onUpdateAvailable((info: any) => {
      useStore.getState().onAutoUpdateAvailable(info?.version ?? null)
    })
    window.loom.onUpdateNotAvailable(() => {
      useStore.getState().onAutoUpdateNotAvailable()
    })
    window.loom.onUpdateDownloadProgress((p) => {
      useStore.getState().onAutoDownloadProgress(p)
    })
    window.loom.onUpdateDownloaded(() => {
      useStore.getState().onAutoUpdateDownloaded()
    })
    window.loom.onUpdateError((msg: string) => {
      useStore.getState().onAutoUpdateError(msg)
    })

    // Listen for engine state changes from main process
    window.loom.onEngineStateChanged((payload) => {
      useStore.getState().setEngineState(payload.state as 'running' | 'stopped' | 'starting')
      if (payload.state === 'running' && payload.port != null) {
        useStore.getState().setPort(payload.port)
      }
    })
    // Hot-reload model config when config directory files change on disk
    window.loom.onModelConfigChanged(() => {
      handleModelsChanged()
    })
  }, [])

  const handleRetry = () => {
    setError(null)
    setReady(false)
    retryCleanupRef.current?.()
    bootstrapApp()
      .then((cleanup) => {
        retryCleanupRef.current = cleanup
        setReady(true)
      })
      .catch((e: any) => setError(e.message || '启动失败'))
  }

  // Error state
  if (error) {
    return (
      <div className={styles.errorBox}>
        <div className={styles.errorInner}>
          <h1 className={styles.errorTitle}>启动失败</h1>
          <p className={styles.errorMessage}>{error}</p>
          <button onClick={handleRetry} className={styles.retryBtn}>
            重试
          </button>
        </div>
      </div>
    )
  }

  // Loading state
  if (!ready) {
    return (
      <div className={styles.loader}>
        <div className={styles.loaderInner}>
          <div className={styles.loaderLogo}>
            <span className={styles.loaderLogoLetter}>L</span>
          </div>
          <h1 className={styles.loaderTitle}>openLoom</h1>
          <div className={styles.loaderStatus}>
            <span className="typing-dots"><span/><span/><span/></span>
          </div>
        </div>
      </div>
    )
  }

  // Onboarding
  if (showOnboarding) {
    return <Onboarding onComplete={() => { window.loom.setPreference('onboarded', true); setShowOnboarding(false) }} />
  }

  // Main app
  return (
    <ErrorBoundary>
      <AppShell>
        <SettingsModal open={settingsOpen} onClose={() => setSettingsOpen(false)} />
        <ToastContainer />
      </AppShell>
      <ConfirmDialog
        open={confirm.open}
        title={confirm.title}
        message={confirm.message}
        danger={confirm.danger}
        onConfirm={() => {
          confirm.resolve?.(true)
          set({ confirm: { open: false, title: '', message: '', danger: false, resolve: null } })
        }}
        onCancel={() => {
          confirm.resolve?.(false)
          set({ confirm: { open: false, title: '', message: '', danger: false, resolve: null } })
        }}
      />
      <UpdateModal />
    </ErrorBoundary>
  )
}
