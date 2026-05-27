import { useEffect, useState } from 'react'
import AppShell from './components/app/AppShell'
import ChatArea from './components/chat/ChatArea'
import InputArea from './components/input/InputArea'
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

        // Check if first launch
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
      .then(() => {
        setReady(true)
      })
      .catch((e: any) => setError(e.message || '启动失败'))
  }

  const handleOnboardingComplete = () => {
    window.hana.setPreference('onboarded', true)
    setShowOnboarding(false)
  }

  // Error state
  if (error) {
    return (
      <div className="flex items-center justify-center min-h-screen bg-zinc-900 text-white">
        <div className="text-center max-w-sm">
          <h1 className="text-2xl font-bold mb-2">启动失败</h1>
          <p className="text-red-400 mb-4 text-sm">{error}</p>
          <button
            onClick={handleRetry}
            className="px-4 py-2 bg-zinc-700 rounded-lg hover:bg-zinc-600 text-sm"
          >
            重试
          </button>
        </div>
      </div>
    )
  }

  // Loading state
  if (!ready) {
    return (
      <div className="flex items-center justify-center min-h-screen bg-zinc-900 text-white">
        <div className="text-center">
          <h1 className="text-3xl font-bold mb-4">openLoom</h1>
          <div className="animate-pulse text-zinc-400 text-sm">正在连接引擎...</div>
        </div>
      </div>
    )
  }

  // Onboarding
  if (showOnboarding) {
    return <Onboarding onComplete={handleOnboardingComplete} />
  }

  // Main app
  return (
    <ErrorBoundary>
      <AppShell>
        <ChatArea />
        <InputArea />
        <SettingsModal open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      </AppShell>
      <ToastContainer />
    </ErrorBoundary>
  )
}
