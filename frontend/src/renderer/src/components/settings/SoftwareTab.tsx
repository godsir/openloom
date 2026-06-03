import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import { type ThemeId, type FontSizeId, FONT_SIZE_MAP } from '../../stores/ui'
import Select, { type SelectOption } from '../shared/Select'
import styles from '../shared/SettingsModal.module.css'

// ── Static font lists (bundled + system fallbacks) ──

/** UI fonts: each option shows its own font face in the dropdown */
const UI_FONT_OPTIONS: SelectOption[] = [
  { value: '', label: '系统默认' },
  {
    value: '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", "Microsoft YaHei", sans-serif',
    label: 'Inter — 现代无衬线',
    fontFamily: 'Inter, sans-serif',
  },
  {
    value: '"Microsoft YaHei", "微软雅黑", "PingFang SC", sans-serif',
    label: '微软雅黑',
    fontFamily: '"Microsoft YaHei", sans-serif',
  },
  {
    value: '"PingFang SC", "苹方", "Microsoft YaHei", sans-serif',
    label: '苹方 (PingFang SC)',
    fontFamily: '"PingFang SC", sans-serif',
  },
  {
    value: '"LXGW WenKai", "霞鹜文楷", "KaiTi", "楷体", serif',
    label: '霞鹜文楷 — 楷体',
    fontFamily: '"LXGW WenKai", "KaiTi", serif',
  },
  {
    value: '"Noto Sans SC", "Microsoft YaHei", sans-serif',
    label: 'Noto Sans SC — 思源黑体',
    fontFamily: '"Noto Sans SC", sans-serif',
  },
]

/** Code / monospace fonts */
const CODE_FONT_OPTIONS: SelectOption[] = [
  { value: '', label: '系统默认' },
  {
    value: '"JetBrains Mono", "Cascadia Code", "Fira Code", "Consolas", monospace',
    label: 'JetBrains Mono',
    fontFamily: '"JetBrains Mono", monospace',
  },
  {
    value: '"Fira Code", "JetBrains Mono", "Cascadia Code", "Consolas", monospace',
    label: 'Fira Code',
    fontFamily: '"Fira Code", monospace',
  },
  {
    value: '"Cascadia Code", "JetBrains Mono", "Fira Code", "Consolas", monospace',
    label: 'Cascadia Code',
    fontFamily: '"Cascadia Code", monospace',
  },
  {
    value: '"IBM Plex Mono", "JetBrains Mono", "Consolas", monospace',
    label: 'IBM Plex Mono',
    fontFamily: '"IBM Plex Mono", monospace',
  },
  {
    value: '"Source Code Pro", "JetBrains Mono", "Fira Code", monospace',
    label: 'Source Code Pro',
    fontFamily: '"Source Code Pro", monospace',
  },
  {
    value: '"Inconsolata", "JetBrains Mono", "Consolas", monospace',
    label: 'Inconsolata',
    fontFamily: 'Inconsolata, monospace',
  },
  {
    value: '"Consolas", "JetBrains Mono", "Fira Code", monospace',
    label: 'Consolas',
    fontFamily: 'Consolas, monospace',
  },
]

export const THEMES: { id: ThemeId; label: string; bg: string; surface: string; text: string; accent: string }[] = [
  { id: 'dark', label: '暗色', bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' },
  { id: 'light', label: '亮色', bg: '#ffffff', surface: '#f1f5f9', text: '#0f172a', accent: '#0d9488' },
  { id: 'midnight', label: '星夜', bg: '#0b1120', surface: '#0f172a', text: '#e2e8f0', accent: '#a5bff8' },
  { id: 'warm-paper', label: '素笺', bg: '#fdfbf7', surface: '#f5f0e8', text: '#2d2416', accent: '#b05a30' },
  { id: 'neon-pink', label: '紫夜', bg: '#1a1a1d', surface: '#222225', text: '#f0e0e8', accent: '#e6397c' },
  { id: 'ember', label: '熔岩', bg: '#000026', surface: '#060630', text: '#ffe0c0', accent: '#ff770f' },
  { id: 'navy-gold', label: '鎏金', bg: '#050F2E', surface: '#0A1A45', text: '#e2e8f0', accent: '#FFE76F' },
  { id: 'umber-cream', label: '摩卡', bg: '#2D1B14', surface: '#3D271D', text: '#fff8f0', accent: '#D8C7B5' },
  { id: 'custom', label: '自定义', bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' },
]

export function hexToRgb(hex: string): [number, number, number] {
  const v = parseInt(hex.replace('#', ''), 16)
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255]
}

export function applyCustomTheme(c: { bg: string; surface: string; text: string; accent: string }) {
  const root = document.documentElement
  const [ar, ag, ab] = hexToRgb(c.accent)
  const isLight = c.bg > '#888'
  const textAlpha = isLight ? 0.08 : 0.08
  const borderBase = isLight ? `rgba(0,0,0,0.06)` : `rgba(255,255,255,0.06)`

  root.style.setProperty('--bg', c.bg)
  root.style.setProperty('--bg-surface', c.surface)
  root.style.setProperty('--bg-card', c.surface)
  root.style.setProperty('--bg-active', isLight ? 'rgba(0,0,0,0.04)' : 'rgba(255,255,255,0.04)')
  root.style.setProperty('--bg-overlay', isLight ? 'rgba(255,255,255,0.70)' : 'rgba(0,0,0,0.72)')
  root.style.setProperty('--bg-input', c.surface)
  root.style.setProperty('--bg-tooltip', c.surface)
  root.style.setProperty('--text', c.text)
  root.style.setProperty('--text-secondary', isLight ? 'rgba(0,0,0,0.55)' : 'rgba(255,255,255,0.55)')
  root.style.setProperty('--text-muted', isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.28)')
  root.style.setProperty('--text-light', isLight ? 'rgba(0,0,0,0.55)' : 'rgba(255,255,255,0.55)')
  root.style.setProperty('--border', borderBase)
  root.style.setProperty('--border-default', isLight ? 'rgba(0,0,0,0.10)' : 'rgba(255,255,255,0.10)')
  root.style.setProperty('--border-accent', `rgba(${ar},${ag},${ab},0.28)`)
  root.style.setProperty('--border-light', `rgba(${ar},${ag},${ab},0.06)`)
  root.style.setProperty('--accent', c.accent)
  root.style.setProperty('--accent-hover', c.accent)
  root.style.setProperty('--accent-rgb', `${ar},${ag},${ab}`)
  root.style.setProperty('--accent-subtle', `rgba(${ar},${ag},${ab},0.10)`)
  root.style.setProperty('--accent-medium', `rgba(${ar},${ag},${ab},0.16)`)
  root.style.setProperty('--accent-glow', `rgba(${ar},${ag},${ab},0.22)`)
  root.style.setProperty('--accent-light', `rgba(${ar},${ag},${ab},0.12)`)
  root.style.setProperty('--accent-strong', `rgba(${ar},${ag},${ab},0.22)`)
  root.style.setProperty('--shadow', isLight ? '0 1px 3px rgba(0,0,0,0.08)' : '0 1px 3px rgba(0,0,0,0.5)')
  root.style.setProperty('--shadow-md', isLight ? '0 4px 16px rgba(0,0,0,0.08)' : '0 4px 16px rgba(0,0,0,0.6)')
  root.style.setProperty('--shadow-lg', isLight ? '0 8px 32px rgba(0,0,0,0.10)' : '0 8px 32px rgba(0,0,0,0.7)')
  root.style.setProperty('--shadow-glass', isLight ? '0 8px 32px rgba(0,0,0,0.06)' : '0 8px 32px rgba(0,0,0,0.5)')
}

export default function SoftwareTab({ theme, setTheme }: { theme: string; setTheme: (t: any) => void }) {
  const fontSize = useStore((s) => s.fontSize)
  const setFontSize = useStore((s) => s.setFontSize)
  const [autoStart, setAutoStart] = useState(false)
  const [closeToTray, setCloseToTray] = useState(true)
  const [autoTitle, setAutoTitle] = useState(false)
  const [uiFont, setUiFont] = useState('')
  const [codeFont, setCodeFont] = useState('')
  const [loaded, setLoaded] = useState(false)
  const [customColors, setCustomColors] = useState({ bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' })

  useEffect(() => {
    Promise.all([
      window.loom.getPreference('autoStart', false),
      window.loom.getPreference('closeToTray', true),
      window.loom.getPreference('autoTitle', false),
      window.loom.getPreference('uiFont', ''),
      window.loom.getPreference('codeFont', ''),
      window.loom.getPreference('customTheme', { bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' }),
    ]).then(([as, ct, at, uf, cf, cc]) => {
      setAutoStart(as)
      setCloseToTray(ct)
      setAutoTitle(at)
      if (uf) document.documentElement.style.setProperty('--font', uf as string)
      if (cf) document.documentElement.style.setProperty('--font-mono', cf as string)
      setUiFont(uf as string)
      setCodeFont(cf as string)
      setCustomColors(cc as typeof customColors)
      setLoaded(true)
    })
  }, [])

  const handleCustomColor = async (key: keyof typeof customColors, val: string) => {
    const next = { ...customColors, [key]: val }
    setCustomColors(next)
    applyCustomTheme(next)
    await window.loom.setPreference('customTheme', next)
  }

  const handleAutoTitle = async (val: boolean) => {
    setAutoTitle(val)
    await window.loom.setPreference('autoTitle', val)
    useStore.getState().addToast({ type: 'success', message: val ? '已开启 AI 自动命名' : '已关闭 AI 自动命名' })
  }

  const handleAutoStart = async (val: boolean) => {
    setAutoStart(val)
    await window.loom.setPreference('autoStart', val)
    useStore.getState().addToast({ type: 'success', message: val ? '已开启开机自启动' : '已关闭开机自启动' })
  }

  const handleCloseToTray = async (val: boolean) => {
    setCloseToTray(val)
    await window.loom.setPreference('closeToTray', val)
    useStore.getState().addToast({ type: 'success', message: val ? '关闭按钮将最小化到托盘' : '关闭按钮将退出程序' })
  }

  const handleUiFont = async (val: string) => {
    setUiFont(val)
    if (val) {
      document.documentElement.style.setProperty('--font', val)
    } else {
      document.documentElement.style.removeProperty('--font')
    }
    await window.loom.setPreference('uiFont', val)
    useStore.getState().addToast({ type: 'success', message: val ? '界面字体已更新' : '已恢复系统默认字体' })
  }

  const handleCodeFont = async (val: string) => {
    setCodeFont(val)
    if (val) {
      document.documentElement.style.setProperty('--font-mono', val)
    } else {
      document.documentElement.style.removeProperty('--font-mono')
    }
    await window.loom.setPreference('codeFont', val)
    useStore.getState().addToast({ type: 'success', message: val ? '代码字体已更新' : '已恢复系统默认等宽字体' })
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>通用</h3>
        <p className={styles.sectionDesc}>外观、字体与软件行为</p>
      </div>
      <div className={styles.contentBody}>
        {/* ── 外观 ── */}
        <div className={styles.aboutSection}>
          <div className={styles.themeLabel}>外观</div>

          {/* Theme */}
          <div className={styles.themeGrid}>
            {THEMES.map((t) => {
              const previewColors = t.id === 'custom' ? customColors : { bg: t.bg, surface: t.surface, text: t.text, accent: t.accent }
              return (
              <button
                key={t.id}
                onClick={() => {
                  setTheme(t.id)
                  if (t.id === 'custom') applyCustomTheme(customColors)
                  useStore.getState().addToast({ type: 'success', message: `主题已切换为${t.label}` })
                }}
                className={`${styles.themeCard} ${theme === t.id ? styles.themeCardActive : ''}`}
              >
                <div
                  className={styles.themePreview}
                  style={{
                    '--pv-bg': previewColors.bg,
                    '--pv-surface': previewColors.surface,
                    '--pv-accent': previewColors.accent,
                    '--pv-text-13': previewColors.text + '22',
                    '--pv-text-27': previewColors.text + '44',
                  } as React.CSSProperties}
                >
                  <div className={styles.themePreviewInner}>
                    <div className={styles.themePreviewSidebar}>
                      <div className={styles.themePreviewAccentBar} />
                      <div className={styles.themePreviewBarWide} />
                      <div className={styles.themePreviewBarNarrow} />
                    </div>
                    <div className={styles.themePreviewMain}>
                      <div>
                        <div className={styles.themePreviewBarTitle} />
                        <div className={styles.themePreviewBarBody} />
                      </div>
                      <div className={styles.themePreviewCard} />
                    </div>
                  </div>
                </div>
                <span className={`${styles.themeName} ${theme === t.id ? styles.themeNameActive : ''}`}>
                  {t.label}
                </span>
              </button>
            )})}
          </div>

          {theme === 'custom' && (
            <div className={styles.customColors}>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.bg }} />
                  背景色
                </label>
                <input
                  type="color"
                  value={customColors.bg}
                  onChange={e => handleCustomColor('bg', e.target.value)}
                  className={styles.customColorInput}
                />
              </div>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.surface }} />
                  表面色
                </label>
                <input
                  type="color"
                  value={customColors.surface}
                  onChange={e => handleCustomColor('surface', e.target.value)}
                  className={styles.customColorInput}
                />
              </div>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.text }} />
                  文字色
                </label>
                <input
                  type="color"
                  value={customColors.text}
                  onChange={e => handleCustomColor('text', e.target.value)}
                  className={styles.customColorInput}
                />
              </div>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.accent }} />
                  强调色
                </label>
                <input
                  type="color"
                  value={customColors.accent}
                  onChange={e => handleCustomColor('accent', e.target.value)}
                  className={styles.customColorInput}
                />
              </div>
            </div>
          )}

          {loaded && (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>字体大小</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>调整对话和输入区域的文字大小</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  {(Object.entries(FONT_SIZE_MAP) as [FontSizeId, { label: string; px: number }][]).map(([id, { label }]) => (
                    <button
                      key={id}
                      className={`${styles.mcpTransportBtn} ${fontSize === id ? styles.mcpTransportActive : ''}`}
                      onClick={() => setFontSize(id)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>
            </>
          )}
        </div>

        <hr className={styles.sectionDivider} />

        {/* ── 字体 ── */}
        <div className={styles.aboutSection}>
          <div className={styles.themeLabel}>字体</div>
          {!loaded ? (
            <p className={styles.toolsEmpty}>加载中...</p>
          ) : (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>界面字体</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>菜单、对话和通用文本的字体</p>
                </div>
                <div style={{ width: 240 }}>
                  <Select
                    value={uiFont}
                    options={UI_FONT_OPTIONS}
                    onChange={(v) => handleUiFont(v)}
                    variant="form"
                  />
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>代码字体</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>代码块、Shell 输出等位置的等宽字体</p>
                </div>
                <div style={{ width: 240 }}>
                  <Select
                    value={codeFont}
                    options={CODE_FONT_OPTIONS}
                    onChange={(v) => handleCodeFont(v)}
                    variant="form"
                  />
                </div>
              </div>
              <div className={styles.fontPreview}>
                <div className={styles.fontPreviewLabel}>预览</div>
                <pre className={styles.fontPreviewCode} style={{ fontFamily: codeFont || undefined }}>
                  {`fn main() {\n  println!("你好，openLoom");\n}`}
                </pre>
              </div>
            </>
          )}
        </div>

        <hr className={styles.sectionDivider} />

        {/* ── 行为 ── */}
        <div className={styles.aboutSection}>
          <div className={styles.themeLabel}>行为</div>
          {!loaded ? (
            <p className={styles.toolsEmpty}>加载中...</p>
          ) : (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>AI 自动命名会话</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>首次对话后由 AI 提取 2-7 字标题</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    onClick={() => handleAutoTitle(true)}
                    className={`${styles.mcpTransportBtn} ${autoTitle ? styles.mcpTransportActive : ''}`}
                  >
                    开启
                  </button>
                  <button
                    onClick={() => handleAutoTitle(false)}
                    className={`${styles.mcpTransportBtn} ${!autoTitle ? styles.mcpTransportActive : ''}`}
                  >
                    关闭
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>关闭按钮行为</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>点击标题栏关闭按钮时的操作</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${closeToTray ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleCloseToTray(true)}
                  >
                    最小化到托盘
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!closeToTray ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleCloseToTray(false)}
                  >
                    退出程序
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>开机自启动</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>系统启动时自动运行 openLoom</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${autoStart ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleAutoStart(true)}
                  >
                    开启
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!autoStart ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleAutoStart(false)}
                  >
                    关闭
                  </button>
                </div>
              </div>
            </>
          )}
        </div>

      </div>
    </>
  )
}
