import { useEffect } from 'react'
import { useStore } from '../../stores'
import Sidebar from './Sidebar'
import StatusBar from './StatusBar'
import WindowControls from './WindowControls'
import ChatWorkspace from '../chat/ChatWorkspace'
import { IconPanelLeftClose, IconPanelLeft } from '../../utils/icons'

export default function AppShell() {
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const currentSessionId = useStore(s => s.currentSessionId)
  const sessions = useStore(s => s.sessions)

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
        e.preventDefault()
        toggleSidebar()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [toggleSidebar])

  const currentTitle = currentSessionId
    ? sessions.find(s => s.path === currentSessionId)?.title || 'openLoom'
    : 'openLoom'

  return (
    <div className="h-screen flex flex-col bg-[var(--bg)]">
      {/* Fused Titlebar — 36px */}
      <header
        data-drag
        className="flex items-center h-[var(--titlebar-h)] shrink-0 bg-[var(--bg)] border-b border-[var(--border)] px-3 z-10"
      >
        {/* Left: sidebar toggle + logo */}
        <div data-no-drag className="flex items-center gap-2.5 flex-shrink-0">
          <button
            onClick={toggleSidebar}
            className="flex items-center justify-center w-6 h-6 rounded-[var(--r-sm)] text-[var(--accent)] hover:bg-[rgba(0,227,199,0.06)] transition-colors"
            title="⌘B 切换侧边栏"
          >
            {sidebarOpen ? <IconPanelLeftClose size={14} /> : <IconPanelLeft size={14} />}
          </button>
          <div className="flex items-center gap-1.5">
            <div className="w-4 h-4 rounded-[3px] bg-[var(--accent)] flex items-center justify-center">
              <span className="text-[8px] font-extrabold text-[var(--bg)]">L</span>
            </div>
            <span className="text-[12px] font-medium text-[var(--text-light)] tracking-tight">
              openLoom
            </span>
          </div>
        </div>

        {/* Center: session title (draggable) */}
        <div className="flex-1 text-center">
          <span className="text-[11px] text-[var(--text-muted)]">{currentTitle}</span>
        </div>

        {/* Right: window controls */}
        <div data-no-drag className="flex items-center flex-shrink-0 -mr-2">
          <WindowControls />
        </div>
      </header>

      {/* Body */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar drawer */}
        <div
          className="shrink-0 overflow-hidden transition-all duration-[200ms] ease-[var(--ease-out)]"
          style={{ width: sidebarOpen ? 'var(--sidebar-w)' : '0px', opacity: sidebarOpen ? 1 : 0 }}
        >
          <Sidebar />
        </div>
        <main data-content className="flex-1 flex flex-col min-w-0 relative bg-[var(--bg)]">
          <ChatWorkspace />
        </main>
      </div>

      {/* StatusBar */}
      <StatusBar />
    </div>
  )
}
