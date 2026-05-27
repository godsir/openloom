import { useState } from 'react'
import { useStore } from '../../stores'
import Overlay from './Overlay'
import Button from './Button'
import AgentConfigPanel from './AgentConfigPanel'
import { type ThemeId } from '../../stores/ui'

const THEMES: { id: ThemeId; label: string }[] = [
  { id: 'dark', label: '暗色' },
  { id: 'light', label: '亮色' },
  { id: 'midnight', label: '午夜蓝' },
  { id: 'warm-paper', label: '暖纸' },
]

type Tab = 'appearance' | 'agent' | 'about'

export default function SettingsModal({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) {
  const theme = useStore((s) => s.theme)
  const setTheme = useStore((s) => s.setTheme)
  const wsState = useStore((s) => s.wsState)
  const [tab, setTab] = useState<Tab>('appearance')

  const tabs: { id: Tab; label: string }[] = [
    { id: 'appearance', label: '外观' },
    { id: 'agent', label: 'Agent' },
    { id: 'about', label: '关于' },
  ]

  return (
    <Overlay open={open} onClose={onClose} title="设置">
      <div className="flex gap-4">
        {/* Tab nav */}
        <div className="w-24 shrink-0 space-y-0.5">
          {tabs.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`w-full text-left px-3 py-1.5 text-sm rounded-md transition-colors ${
                tab === t.id
                  ? 'bg-zinc-700 text-zinc-100'
                  : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800'
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* Tab content */}
        <div className="flex-1 min-w-0">
          {tab === 'appearance' && (
            <div className="space-y-4">
              <h3 className="text-sm font-semibold text-zinc-200">主题</h3>
              <div className="flex gap-2 flex-wrap">
                {THEMES.map((t) => (
                  <Button
                    key={t.id}
                    size="sm"
                    variant={theme === t.id ? 'primary' : 'secondary'}
                    onClick={() => setTheme(t.id)}
                  >
                    {t.label}
                  </Button>
                ))}
              </div>
            </div>
          )}

          {tab === 'agent' && <AgentConfigPanel />}

          {tab === 'about' && (
            <div className="space-y-2 text-sm text-zinc-400">
              <p>
                <span className="text-zinc-500">版本</span>{' '}
                <span className="text-zinc-300">openLoom v0.2.0</span>
              </p>
              <p>
                <span className="text-zinc-500">连接状态</span>{' '}
                <span
                  className={
                    wsState === 'connected'
                      ? 'text-green-400'
                      : 'text-yellow-400'
                  }
                >
                  {wsState === 'connected' ? '已连接' : wsState}
                </span>
              </p>
              <p className="text-xs text-zinc-600 mt-4">
                本地优先的私人 AI 助理。所有数据存储在本地。
              </p>
            </div>
          )}
        </div>
      </div>
    </Overlay>
  )
}
