import { type ReactNode } from 'react'
import Sidebar from './Sidebar'
import StatusBar from './StatusBar'
import WindowControls from './WindowControls'
import SettingsModal from '../shared/SettingsModal'
import { useStore } from '../../stores'

export default function AppShell({ children }: { children: ReactNode }) {
  const settingsOpen = useStore((s) => s.settingsOpen)
  const setSettingsOpen = useStore((s) => s.setSettingsOpen)

  return (
    <div className="flex flex-col h-screen">
      <div
        className="flex items-center justify-between h-8 bg-zinc-950 border-b border-zinc-800 shrink-0"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      >
        <WindowControls />
        <span className="text-xs text-zinc-600 mr-3">openLoom</span>
      </div>

      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main className="flex-1 flex flex-col overflow-hidden">{children}</main>
      </div>

      <StatusBar />

      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
    </div>
  )
}
