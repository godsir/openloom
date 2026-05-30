import { useEffect, useRef, useState } from 'react'
import AppShell from './components/app/AppShell'
import SettingsModal from './components/shared/SettingsModal'

import Onboarding from './components/shared/Onboarding'
import ErrorBoundary from './components/shared/ErrorBoundary'
import ToastContainer from './components/shared/ToastContainer'
import ConfirmDialog from './components/shared/ConfirmDialog'
import { bootstrapApp } from './services/bootstrap'
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
        const pref = await window.hana.getPreference('onboarded', false)
        if (!pref) setShowOnboarding(true)
        const savedTheme = await window.hana.getPreference('theme', 'dark')
        useStore.getState().setTheme(savedTheme as any)
        const savedFontSize = await window.hana.getPreference('fontSize', 'default')
        useStore.getState().setFontSize(savedFontSize as any)
        const savedPinned = await window.hana.getPreference<string[]>('pinnedIds', [])
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
    return <Onboarding onComplete={() => { window.hana.setPreference('onboarded', true); setShowOnboarding(false) }} />
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
    </ErrorBoundary>
  )
}
