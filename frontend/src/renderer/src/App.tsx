import { useEffect, useRef, useState } from 'react'
import AppShell from './components/app/AppShell'
import ScheduledTasksModal from './components/shared/ScheduledTasksModal'
import CronDetectedDialog from './components/shared/CronDetectedDialog'
import UpdateModal from './components/shared/UpdateModal'

import Onboarding from './components/shared/Onboarding'
import ErrorBoundary from './components/shared/ErrorBoundary'
import ToastContainer from './components/shared/ToastContainer'
import ConfirmDialog from './components/shared/ConfirmDialog'
import PermissionDialog from './components/shared/PermissionDialog'
import { InlineInput } from './components/input/InlineInput'
import { bootstrapApp } from './services/bootstrap'
import { handleModelsChanged } from './services/app-event-actions'
import { keybindingRegistry } from './services/keybindings'
import { loomRpc } from './services/jsonrpc'
import { rpc } from './services/rpc-toast'
import { useStore } from './stores'
import { t } from './i18n'
import type { PermissionMode } from './stores/input'
import styles from './App.module.css'

export default function App() {
  const [ready, setReady] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [showOnboarding, setShowOnboarding] = useState(false)
  const storeShowOnboarding = useStore((s) => s.showOnboarding)
  const setStoreShowOnboarding = useStore((s) => s.setShowOnboarding)
  const retryCleanupRef = useRef<(() => void) | null>(null)
  const scheduledTasksOpen = useStore((s) => s.scheduledTasksOpen)
  const setScheduledTasksOpen = useStore((s) => s.setScheduledTasksOpen)
  const theme = useStore((s) => s.theme)
  const confirm = useStore((s) => s.confirm)
  const permissionConfirm = useStore((s) => s.permissionConfirm)
  const cronDetected = useStore((s) => s.cronDetected)
  const set = useStore.setState

  // Apply theme on mount and on change
  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme)
  }, [theme])

  // Set platform attribute for CSS targeting (Win11 animation fix)
  useEffect(() => {
    window.loom.getPlatform().then((platform: string) => {
      document.documentElement.setAttribute('data-platform', platform)
    })
  }, [])

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
        const savedZoom = await window.loom.getPreference('appZoom', 1)
        document.documentElement.style.setProperty('--app-zoom', String(savedZoom))
        const savedSendShortcut = await window.loom.getPreference('sendShortcut', 'enter')
        useStore.getState().setSendShortcut(savedSendShortcut as any)
        const savedPermissionMode = await window.loom.getPreference('permissionMode', 'ask')
        const validModes = ['operate', 'ask', 'read_only', 'plan']
        if (validModes.includes(String(savedPermissionMode))) {
          useStore.getState().setPermissionMode(savedPermissionMode as PermissionMode)
        }
        const savedFimEnabled = await window.loom.getPreference('fimEnabled', false)
        useStore.getState().setFimEnabled(savedFimEnabled as boolean)
        const savedThinkingLevel = await window.loom.getPreference('thinkingLevel', 'medium')
        useStore.getState().setThinkingLevel(savedThinkingLevel as any)
        const savedWorkBlockExpand = await window.loom.getPreference('workBlockExpandDefault', true)
        useStore.setState({ workBlockExpandDefault: Boolean(savedWorkBlockExpand) })
        const savedPinned = await window.loom.getPreference<string[]>('pinnedIds', [])
        if (savedPinned.length) {
          useStore.setState({ pinnedIds: new Set(savedPinned) })
        }
        // Apply saved fonts on startup
        const savedUiFont = await window.loom.getPreference('uiFont', '')
        const savedCodeFont = await window.loom.getPreference('codeFont', '')
        const root = document.documentElement
        if (savedUiFont) {
          root.style.setProperty('--font', savedUiFont as string)
          if ((savedUiFont as string).includes('KaiTi') || (savedUiFont as string).includes('楷体')) {
            root.style.setProperty('-webkit-text-stroke', '0.35px')
          }
        }
        if (savedCodeFont) {
          root.style.setProperty('--font-mono', savedCodeFont as string)
        }
      } catch (e: any) {
        if (cancelled) return
        setError(e.message || t('error.startupFailed'))
      }
    }
    boot()
    return () => { cancelled = true; teardown?.() }
  }, [])

  // Global update IPC listeners (registered once on mount)
  useEffect(() => {
    const disposers: Array<() => void> = []
    disposers.push(window.loom.onUpdateAvailable((info: any) => {
      useStore.getState().onAutoUpdateAvailable(info?.version ?? null, info?.releaseNotes ?? null)
    }))
    disposers.push(window.loom.onUpdateNotAvailable(() => {
      useStore.getState().onAutoUpdateNotAvailable()
    }))
    disposers.push(window.loom.onUpdateDownloadProgress((p: any) => {
      useStore.getState().onAutoDownloadProgress(p)
    }))
    disposers.push(window.loom.onUpdateDownloaded(() => {
      useStore.getState().onAutoUpdateDownloaded()
    }))
    disposers.push(window.loom.onUpdateDownloadCancelled(() => {
      useStore.getState().onAutoDownloadCancelled()
    }))
    disposers.push(window.loom.onUpdateError((msg: string) => {
      useStore.getState().onAutoUpdateError(msg)
    }))

    // Listen for engine state changes from main process
    disposers.push(window.loom.onEngineStateChanged((payload: { state: string; port?: number }) => {
      useStore.getState().setEngineState(payload.state as 'running' | 'stopped' | 'starting')
      if (payload.state === 'running' && payload.port != null) {
        useStore.getState().setPort(payload.port)
      }
    }))
    // Hot-reload model config when config directory files change on disk
    disposers.push(window.loom.onModelConfigChanged(() => {
      handleModelsChanged()
    }))
    // Navigate from tray menu
    disposers.push(window.loom.onNavigate((route: string) => {
      if (route === '/settings') {
        useStore.getState().setAppMode('settings')
      }
    }))

    return () => { for (const dispose of disposers) dispose() }
  }, [])

  // Global keyboard shortcuts via KeybindingRegistry
  // Replaces old hardcoded Ctrl+B (AppShell) and Ctrl+Shift+I listeners
  useEffect(() => {
    // Populate default commands synchronously so dispatch works immediately.
    // Custom overrides load asynchronously and are applied when ready.
    keybindingRegistry.initialize()

    // Navigation commands
    keybindingRegistry.register('nav:new-conversation', () => {
      useStore.getState().createSession()
    })
    keybindingRegistry.register('nav:close-conversation', () => {
      useStore.getState().closeCurrentSession()
    })
    keybindingRegistry.register('nav:next-conversation', () => {
      useStore.getState().selectNextSession()
    })
    keybindingRegistry.register('nav:prev-conversation', () => {
      useStore.getState().selectPrevSession()
    })
    keybindingRegistry.register('nav:search-conversations', () => {
      const sidebarSearch = document.querySelector<HTMLInputElement>(
        '.sidebarSearch input, [data-sidebar-search]'
      )
      sidebarSearch?.focus()
    })
    keybindingRegistry.register('nav:focus-input', () => {
      const chatInput = document.querySelector<HTMLTextAreaElement>(
        'textarea[data-chat-input]'
      )
      chatInput?.focus()
    })
    keybindingRegistry.register('nav:switch-workspace', async () => {
      const sessionId = useStore.getState().currentSessionId
      if (!sessionId) return
      const path = await window.loom.selectFolder()
      if (path) {
        useStore.getState().setSessionWorkspace(sessionId, path)
        await rpc('workspace.set_session', { session_id: sessionId, path }, t('sidebar.setWorkspace'))
      }
    })

    // UI commands
    keybindingRegistry.register('ui:toggle-sidebar', () => {
      const s = useStore.getState()
      if (s.appMode === 'write') {
        s.toggleWriteFileSidebar()
      } else {
        s.toggleSidebar()
      }
    })
    keybindingRegistry.register('ui:open-settings', () => {
      useStore.setState({ appMode: 'settings' })
    })
    keybindingRegistry.register('ui:toggle-mode', () => {
      const s = useStore.getState()
      if (s.appMode === 'chat') {
        useStore.setState({ appMode: 'write' })
      } else if (s.appMode === 'write') {
        useStore.setState({ appMode: 'chat' })
      }
    })
    keybindingRegistry.register('ui:zoom-in', () => {
      const current = parseFloat(
        document.documentElement.style.getPropertyValue('--app-zoom') || '1'
      )
      const next = Math.min(current + 0.1, 2.0)
      document.documentElement.style.setProperty('--app-zoom', next.toFixed(2))
      window.loom.setPreference('appZoom', next)
    })
    keybindingRegistry.register('ui:zoom-out', () => {
      const current = parseFloat(
        document.documentElement.style.getPropertyValue('--app-zoom') || '1'
      )
      const next = Math.max(current - 0.1, 0.5)
      document.documentElement.style.setProperty('--app-zoom', next.toFixed(2))
      window.loom.setPreference('appZoom', next)
    })
    keybindingRegistry.register('ui:zoom-reset', () => {
      document.documentElement.style.setProperty('--app-zoom', '1')
      window.loom.setPreference('appZoom', 1)
    })

    // Inline selection editor (still Ctrl+Shift+I)
    keybindingRegistry.register('ui:inline-edit', () => {
      const sel = window.getSelection()
      if (!sel || sel.isCollapsed || !sel.toString().trim()) return

      const range = sel.getRangeAt(0)
      const rect = range.getBoundingClientRect()

      let filePath = ''
      let startLine = 0
      let endLine = 0
      let el = range.startContainer.parentElement
      while (el) {
        const fp = el.getAttribute('data-file-path')
        if (fp) {
          filePath = fp
          const sl = el.getAttribute('data-start-line')
          const el2 = el.getAttribute('data-end-line')
          if (sl) startLine = parseInt(sl, 10)
          if (el2) endLine = parseInt(el2, 10)
          break
        }
        el = el.parentElement
      }

      useStore.getState().openInlineInput(rect, filePath, startLine, endLine)
    })

    // Global keydown handler (capture phase)
    const handler = (e: KeyboardEvent) => {
      keybindingRegistry.dispatch(e)
    }
    window.addEventListener('keydown', handler, true)
    return () => window.removeEventListener('keydown', handler, true)
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
      .catch((e: any) => setError(e.message || t('error.startupFailed')))
  }

  // Error state
  if (error) {
    return (
      <div className={styles.errorBox}>
        <div className={styles.errorInner}>
          <h1 className={styles.errorTitle}>{t('error.startupFailed')}</h1>
          <p className={styles.errorMessage}>{error}</p>
          <button onClick={handleRetry} className={styles.retryBtn}>
            {t('common.retry')}
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
  if (showOnboarding || storeShowOnboarding) {
    return <Onboarding onComplete={() => { window.loom.setPreference('onboarded', true); setShowOnboarding(false); setStoreShowOnboarding(false) }} />
  }

  // Main app
  return (
    <ErrorBoundary>
      <AppShell>
        <ScheduledTasksModal open={scheduledTasksOpen} onClose={() => setScheduledTasksOpen(false)} />
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
      <PermissionDialog
        open={permissionConfirm.open}
        title={permissionConfirm.title}
        message={permissionConfirm.message}
        toolName={permissionConfirm.toolName}
        danger={permissionConfirm.danger}
        onApprove={() => {
          permissionConfirm.resolve?.('approve')
          set({ permissionConfirm: { open: false, title: '', message: '', danger: false, toolName: '', resolve: null } })
        }}
        onApproveAlways={() => {
          permissionConfirm.resolve?.('approve_always')
          set({ permissionConfirm: { open: false, title: '', message: '', danger: false, toolName: '', resolve: null } })
        }}
        onDeny={() => {
          permissionConfirm.resolve?.('deny')
          set({ permissionConfirm: { open: false, title: '', message: '', danger: false, toolName: '', resolve: null } })
        }}
      />
      <CronDetectedDialog
        open={cronDetected.open}
        name={cronDetected.name}
        prompt={cronDetected.prompt}
        cronExpression={cronDetected.cronExpression}
        kind={cronDetected.kind}
        confirmation={cronDetected.confirmation}
        onCreate={() => {
          const resolve = cronDetected.resolve
          set({ cronDetected: { open: false, name: '', prompt: '', cronExpression: '', kind: '', confirmation: '', resolve: null } })
          resolve?.(true)
        }}
        onCancel={() => {
          const resolve = cronDetected.resolve
          set({ cronDetected: { open: false, name: '', prompt: '', cronExpression: '', kind: '', confirmation: '', resolve: null } })
          resolve?.(false)
        }}
      />
      <UpdateModal />
      <InlineInput />
    </ErrorBoundary>
  )
}
