import { useEffect, useState } from 'react'
import AppShell from './components/app/AppShell'
import SettingsModal from './components/shared/SettingsModal'
import WelcomeScreen from './components/shared/WelcomeScreen'
import Onboarding from './components/shared/Onboarding'
import ErrorBoundary from './components/shared/ErrorBoundary'
import ToastContainer from './components/shared/ToastContainer'
import { bootstrapApp } from './services/bootstrap'
import { useStore } from './stores'

export default function App() {
  const [ready, setReady] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [showOnboarding, setShowOnboarding] = useState(false)
  const settingsOpen = useStore((s) => s.settingsOpen)
  const setSettingsOpen = useStore((s) => s.setSettingsOpen)

  useEffect(() => {
    let cancelled = false
    async function boot() {
      try {
        await bootstrapApp()
        if (cancelled) return
        setReady(true)
        const pref = await window.hana.getPreference('onboarded', false)
        if (!pref) setShowOnboarding(true)
      } catch (e: any) {
        if (cancelled) return
        setError(e.message || '启动失败')
      }
    }
    boot()
    return () => { cancelled = true }
  }, [])

  const handleRetry = () => {
    setError(null)
    setReady(false)
    bootstrapApp()
      .then(() => setReady(true))
      .catch((e: any) => setError(e.message || '启动失败'))
  }

  // Error state
  if (error) {
    return (
      <div className="flex items-center justify-center h-screen bg-[var(--bg)]">
        <div className="text-center max-w-sm animate-fade-in">
          <h1 className="text-2xl font-semibold text-[var(--text)] mb-3">启动失败</h1>
          <p className="text-[var(--red)] mb-5 text-sm">{error}</p>
          <button onClick={handleRetry}
            className="px-5 py-2 rounded-[var(--r-sm)] bg-[var(--bg-card)] text-[var(--text-light)] hover:bg-[rgba(255,255,255,0.04)] border border-[var(--border)] text-sm transition-colors">
            重试
          </button>
        </div>
      </div>
    )
  }

  // Loading state
  if (!ready) {
    return (
      <div className="flex items-center justify-center h-screen bg-[var(--bg)]">
        <div className="text-center animate-fade-in">
          <div className="w-20 h-20 mx-auto mb-6 rounded-[var(--r-sm)] bg-[var(--accent-light)] border border-[rgba(var(--accent-rgb),.15)] flex items-center justify-center shadow-[var(--shadow-glow)] animate-breathe">
            <span className="text-3xl font-bold text-[var(--accent)]">L</span>
          </div>
          <h1 className="text-[32px] font-semibold text-[var(--text)] tracking-tight">
            openLoom
          </h1>
          <div className="flex items-center gap-2 justify-center mt-4 text-sm text-[var(--text-muted)]">
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
      <AppShell />
      <SettingsModal open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <ToastContainer />
    </ErrorBoundary>
  )
}
