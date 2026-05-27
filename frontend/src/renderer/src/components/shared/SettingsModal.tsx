import { useState } from 'react'
import { useStore } from '../../stores'
import Overlay from './Overlay'
import Button from './Button'
import AgentConfigPanel from './AgentConfigPanel'
import ModelConfigPanel from './ModelConfigPanel'
import { type ThemeId } from '../../stores/ui'

const THEMES: { id: ThemeId; label: string }[] = [
  { id: 'dark', label: '暗色' },
  { id: 'light', label: '亮色' },
  { id: 'midnight', label: '午夜蓝' },
  { id: 'warm-paper', label: '暖纸' },
]

type Tab = 'appearance' | 'agent' | 'models' | 'about'

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
    { id: 'models', label: '模型' },
    { id: 'about', label: '关于' },
  ]

  return (
    <Overlay open={open} onClose={onClose} title="设置">
      <div className="flex gap-5">
        <div className="w-20 shrink-0 space-y-0.5">
          {tabs.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`w-full text-left px-3 py-1.5 text-[13px] rounded-[var(--r-sm)] transition-colors-fast ${
                tab === t.id
                  ? 'bg-[var(--accent-light)] text-[var(--accent)]'
                  : 'text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-card)]'
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>

        <div className="flex-1 min-w-0">
          {tab === 'appearance' && (
            <div className="space-y-4">
              <h3 className="text-sm font-semibold text-[var(--text)]">主题</h3>
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

          {tab === 'models' && <ModelConfigPanel />}

          {tab === 'about' && (
            <div className="space-y-2.5 text-[13px] text-[var(--text-muted)]">
              <p>
                <span className="text-[var(--text-muted)]">版本</span>{' '}
                <span className="font-mono text-[var(--text-light)]">openLoom v0.2.0</span>
              </p>
              <p>
                <span className="text-[var(--text-muted)]">连接状态</span>{' '}
                <span
                  className={`font-mono ${
                    wsState === 'connected'
                      ? 'text-[var(--green)]'
                      : 'text-[var(--amber)]'
                  }`}
                >
                  {wsState === 'connected' ? '已连接' : wsState}
                </span>
              </p>
              <p className="text-xs text-[var(--text-muted)] mt-4 leading-relaxed">
                本地优先的私人 AI 助理。所有数据存储在本地。
              </p>
            </div>
          )}
        </div>
      </div>
    </Overlay>
  )
}
