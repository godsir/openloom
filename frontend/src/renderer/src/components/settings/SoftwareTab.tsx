import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import { type ThemeId, type FontSizeId, FONT_SIZE_MAP } from '../../stores/ui'
import type { SendShortcut } from '../../stores/input'
import { useLocale, t as _t, LOCALES } from '../../i18n'
import type { Locale } from '../../i18n'
import { loomRpc } from '../../services/jsonrpc'
import Select, { type SelectOption } from '../shared/Select'
import styles from '../shared/SettingsModal.module.css'
import { readThemeColors } from '../../utils/theme'

// ── Static font lists (bundled + system fallbacks) ──

function useUiFontOptions(): SelectOption[] {
  return [
    { value: '', label: _t('software.systemDefault') },
    {
      value: '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", "Microsoft YaHei", sans-serif',
      label: _t('software.fontInter'),
      fontFamily: 'Inter, sans-serif',
    },
    {
      value: '"Microsoft YaHei", "微软雅黑", "PingFang SC", sans-serif',
      label: _t('software.fontYahei'),
      fontFamily: '"Microsoft YaHei", sans-serif',
    },
    {
      value: '"PingFang SC", "苹方", "Microsoft YaHei", sans-serif',
      label: _t('software.fontPingfang'),
      fontFamily: '"PingFang SC", sans-serif',
    },
    {
      value: '"LXGW WenKai", "霞鹜文楷", "KaiTi", "楷体", serif',
      label: _t('software.fontWenkai'),
      fontFamily: '"LXGW WenKai", "KaiTi", serif',
    },
    {
      value: '"Noto Sans SC", "Microsoft YaHei", sans-serif',
      label: _t('software.fontNotosans'),
      fontFamily: '"Noto Sans SC", sans-serif',
    },
  ]
}

function useCodeFontOptions(): SelectOption[] {
  return [
    { value: '', label: _t('software.systemDefault') },
    {
      value: '"JetBrains Mono", "Cascadia Code", "Fira Code", "Consolas", monospace',
      label: _t('software.fontJetbrains'),
      fontFamily: '"JetBrains Mono", monospace',
    },
    {
      value: '"Fira Code", "JetBrains Mono", "Cascadia Code", "Consolas", monospace',
      label: _t('software.fontFiracode'),
      fontFamily: '"Fira Code", monospace',
    },
    {
      value: '"Cascadia Code", "JetBrains Mono", "Fira Code", "Consolas", monospace',
      label: _t('software.fontCascadia'),
      fontFamily: '"Cascadia Code", monospace',
    },
    {
      value: '"IBM Plex Mono", "JetBrains Mono", "Consolas", monospace',
      label: _t('software.fontIbmplex'),
      fontFamily: '"IBM Plex Mono", monospace',
    },
    {
      value: '"Source Code Pro", "JetBrains Mono", "Fira Code", monospace',
      label: _t('software.fontSourcecode'),
      fontFamily: '"Source Code Pro", monospace',
    },
    {
      value: '"Inconsolata", "JetBrains Mono", "Consolas", monospace',
      label: _t('software.fontInconsolata'),
      fontFamily: 'Inconsolata, monospace',
    },
    {
      value: '"Consolas", "JetBrains Mono", "Fira Code", monospace',
      label: _t('software.fontConsolas'),
      fontFamily: 'Consolas, monospace',
    },
  ]
}

export const THEMES: { id: ThemeId; label: string }[] = [
  { id: 'dark', label: _t('theme.dark') },
  { id: 'light', label: _t('theme.light') },
  { id: 'midnight', label: _t('theme.midnight') },
  { id: 'warm-paper', label: _t('theme.warmPaper') },
  { id: 'neon-pink', label: _t('theme.neonPink') },
  { id: 'ember', label: _t('theme.ember') },
  { id: 'navy-gold', label: _t('theme.navyGold') },
  { id: 'umber-cream', label: _t('theme.umberCream') },
  { id: 'custom', label: _t('theme.custom') },
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
  const { t, locale, setLocale } = useLocale()
  const fontSize = useStore((s) => s.fontSize)
  const setFontSize = useStore((s) => s.setFontSize)
  const sendShortcut = useStore((s) => s.sendShortcut)
  const [autoStart, setAutoStart] = useState(false)
  const [closeToTray, setCloseToTray] = useState(true)
  const [startToTray, setStartToTray] = useState(false)
  const [autoTitle, setAutoTitle] = useState(true)
  const [uiFont, setUiFont] = useState('')
  const [codeFont, setCodeFont] = useState('')
  const [disableHwAccel, setDisableHwAccel] = useState(false)
  const [taskCompleteNotification, setTaskCompleteNotification] = useState(false)
  const [thinkingExpand, setThinkingExpand] = useState(false)
  const [toolExpand, setToolExpand] = useState(true)
  const [skillExpand, setSkillExpand] = useState(false)
  const [useSystemProxy, setUseSystemProxy] = useState(false)
  const [isWin32, setIsWin32] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [activeSection, setActiveSection] = useState('chat')
  const [customColors, setCustomColors] = useState({ bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' })
  const UI_FONT_OPTIONS = useUiFontOptions()
  const CODE_FONT_OPTIONS = useCodeFontOptions()

  const SECTIONS = [
    { id: 'chat', label: t('software.chatSettings') },
    { id: 'behavior', label: t('software.behavior') },
    { id: 'font', label: t('software.font') },
    { id: 'appearance', label: t('software.appearance') },
  ]

  const scrollToSection = (id: string) => {
    const el = document.getElementById(`sw-section-${id}`)
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'start' })
      setActiveSection(id)
    }
  }

  // Scroll spy: track which section is visible
  useEffect(() => {
    const contentEl = document.getElementById('sw-content-body')
    if (!contentEl) return
    const sectionEls = SECTIONS.map(s => document.getElementById(`sw-section-${s.id}`)).filter(Boolean) as HTMLElement[]
    if (sectionEls.length === 0) return

    const onScroll = () => {
      const top = contentEl.scrollTop + 80 // offset for sub-nav
      for (let i = sectionEls.length - 1; i >= 0; i--) {
        if (sectionEls[i].offsetTop <= top) {
          setActiveSection(SECTIONS[i].id)
          break
        }
      }
    }
    contentEl.addEventListener('scroll', onScroll, { passive: true })
    return () => contentEl.removeEventListener('scroll', onScroll)
  }, [SECTIONS])

  useEffect(() => {
    Promise.all([
      window.loom.getPreference('autoStart', false),
      window.loom.getPreference('closeToTray', true),
      window.loom.getPreference('startToTray', false),
      window.loom.getPreference('autoTitle', true),
      window.loom.getPreference('uiFont', ''),
      window.loom.getPreference('codeFont', ''),
      window.loom.getPreference('customTheme', { bg: '#0B0F14', surface: '#111820', text: '#e2e8f0', accent: '#22d3ee' }),
      window.loom.getPreference('disableHardwareAcceleration', false),
      window.loom.getPreference('thinkingExpandDefault', false),
      window.loom.getPreference('toolExpandDefault', true),
      window.loom.getPreference('skillExpandDefault', false),
      window.loom.getPreference('taskCompleteNotification', false),
      loomRpc<{ http_proxy?: string }>('config.get_tool_prefs').then(p => {
        // 没有自定义代理 → 使用系统代理（默认）
        setUseSystemProxy(!p.http_proxy)
      }).catch(() => {}),
      window.loom.getPlatform(),
    ]).then(([as, ct, st, at, uf, cf, cc, dha, te, toe, se, tcn, plat]) => {
      setAutoStart(as)
      setCloseToTray(ct)
      setStartToTray(st)
      setAutoTitle(at)
      if (uf) document.documentElement.style.setProperty('--font', uf as string)
      if (cf) document.documentElement.style.setProperty('--font-mono', cf as string)
      if ((uf as string).includes('KaiTi') || (uf as string).includes('楷体')) {
        document.documentElement.style.setProperty('-webkit-text-stroke', '0.35px')
      }
      setUiFont(uf as string)
      setCodeFont(cf as string)
      setCustomColors(cc as typeof customColors)
      setDisableHwAccel(dha as boolean)
      setThinkingExpand(te as boolean)
      setToolExpand(toe as boolean)
      setSkillExpand(se as boolean)
      setTaskCompleteNotification(tcn as boolean)
      setIsWin32(plat === 'win32')
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
    useStore.getState().addToast({ type: 'success', message: val ? t('software.autoNameEnabled') : t('software.autoNameDisabled') })
  }

  const handleAutoStart = async (val: boolean) => {
    setAutoStart(val)
    await window.loom.setPreference('autoStart', val)
    useStore.getState().addToast({ type: 'success', message: val ? t('software.autoStartEnabled') : t('software.autoStartDisabled') })
  }

  const handleStartToTray = async (val: boolean) => {
    setStartToTray(val)
    await window.loom.setPreference('startToTray', val)
    useStore.getState().addToast({ type: 'success', message: val ? t('software.startToTrayOnToast') : t('software.startToTrayOffToast') })
  }

  const handleCloseToTray = async (val: boolean) => {
    setCloseToTray(val)
    await window.loom.setPreference('closeToTray', val)
    useStore.getState().addToast({ type: 'success', message: val ? t('software.minimizeToTrayToast') : t('software.quitAppToast') })
  }

  const handleDisableHwAccel = async (val: boolean) => {
    setDisableHwAccel(val)
    await window.loom.setPreference('disableHardwareAcceleration', val)
    useStore.getState().addToast({
      type: 'success',
      message: val ? t('software.hwAccelDisabled') : t('software.hwAccelEnabled'),
    })
  }

  const handleTaskCompleteNotification = async (val: boolean) => {
    setTaskCompleteNotification(val)
    await window.loom.setPreference('taskCompleteNotification', val)
    useStore.getState().addToast({
      type: 'success',
      message: val ? t('software.taskNotifEnabled') : t('software.taskNotifDisabled'),
    })
  }

  const handleThinkingExpand = async (val: boolean) => {
    setThinkingExpand(val)
    await window.loom.setPreference('thinkingExpandDefault', val)
    useStore.getState().addToast({ type: 'success', message: val ? t('software.thinkingExpanded') : t('software.thinkingCollapsed') })
  }

  const handleToolExpand = async (val: boolean) => {
    setToolExpand(val)
    await window.loom.setPreference('toolExpandDefault', val)
    useStore.getState().addToast({ type: 'success', message: val ? t('software.toolExpanded') : t('software.toolCollapsed') })
  }

  const handleSkillExpand = async (val: boolean) => {
    setSkillExpand(val)
    await window.loom.setPreference('skillExpandDefault', val)
    useStore.getState().addToast({ type: 'success', message: val ? t('software.skillExpanded') : t('software.skillCollapsed') })
  }

  const handleSendShortcut = (val: string) => {
    useStore.getState().setSendShortcut(val as SendShortcut)
    const labels: Record<string, string> = {
      enter: t('software.sendShortcutEnter'),
      'ctrl+enter': t('software.sendShortcutCtrlEnter'),
      'shift+enter': t('software.sendShortcutShiftEnter'),
    }
    useStore.getState().addToast({ type: 'success', message: _t('software.sendShortcutChanged', { label: labels[val] ?? val }) })
  }

  const handleUiFont = async (val: string) => {
    setUiFont(val)
    if (val) {
      document.documentElement.style.setProperty('--font', val)
      // KaiTi is thinner than other CJK fonts — micro-stroke for readability
      if (val.includes('KaiTi') || val.includes('楷体')) {
        document.documentElement.style.setProperty('-webkit-text-stroke', '0.35px')
      } else {
        document.documentElement.style.removeProperty('-webkit-text-stroke')
      }
    } else {
      document.documentElement.style.removeProperty('--font')
      document.documentElement.style.removeProperty('-webkit-text-stroke')
    }
    await window.loom.setPreference('uiFont', val)
    useStore.getState().addToast({ type: 'success', message: val ? t('software.uiFontUpdated') : t('software.uiFontReset') })
  }

  const handleCodeFont = async (val: string) => {
    setCodeFont(val)
    if (val) {
      document.documentElement.style.setProperty('--font-mono', val)
    } else {
      document.documentElement.style.removeProperty('--font-mono')
    }
    await window.loom.setPreference('codeFont', val)
    useStore.getState().addToast({ type: 'success', message: val ? t('software.codeFontUpdated') : t('software.codeFontReset') })
  }

  const handleResetSize = async () => {
    // Reset font size to default (14px) — persisted via store action
    useStore.getState().setFontSize('default')
    setFontSize('default')

    // Reset UI font to system default — persisted
    setUiFont('')
    document.documentElement.style.removeProperty('--font')
    document.documentElement.style.removeProperty('-webkit-text-stroke')
    window.loom.setPreference('uiFont', '')

    // Reset code font to system default — persisted
    setCodeFont('')
    document.documentElement.style.removeProperty('--font-mono')
    window.loom.setPreference('codeFont', '')

    // Reset Electron webContents zoom (Ctrl+/- zoom) to 1.0 — persisted
    window.loom.setZoomFactor(1.0)

    useStore.getState().addToast({ type: 'success', message: t('software.resetComplete') })
  }

  return (
    <>
      <div className={styles.contentHeader}>
        <h3 className={styles.sectionTitle}>{t('settings.software')}</h3>
        <p className={styles.sectionDesc}>{t('software.description')}</p>
      </div>
      <div className={styles.contentBody} id="sw-content-body">
        {/* Quick-jump sub nav */}
        <div className={styles.subNav}>
          {SECTIONS.map(s => (
            <button
              key={s.id}
              onClick={() => scrollToSection(s.id)}
              className={`${styles.subNavItem} ${activeSection === s.id ? styles.subNavActive : ''}`}
            >
              {s.label}
            </button>
          ))}
        </div>

        {/* Chat Settings */}
        <div className={styles.aboutSection} id="sw-section-chat">
          <div className={styles.themeLabel}>{t('software.chatSettings')}</div>
          {!loaded ? (
            <p className={styles.toolsEmpty}>{t('common.loading')}</p>
          ) : (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.thinkingExpand')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.thinkingExpandDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${thinkingExpand ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleThinkingExpand(true)}
                  >
                    {t('software.expand')}
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!thinkingExpand ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleThinkingExpand(false)}
                  >
                    {t('software.collapse')}
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.toolExpand')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.toolExpandDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${toolExpand ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleToolExpand(true)}
                  >
                    {t('software.expand')}
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!toolExpand ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleToolExpand(false)}
                  >
                    {t('software.collapse')}
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.skillExpand')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.skillExpandDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${skillExpand ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleSkillExpand(true)}
                  >
                    {t('software.expand')}
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!skillExpand ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleSkillExpand(false)}
                  >
                    {t('software.collapse')}
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.sendShortcut')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.sendShortcutDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${sendShortcut === 'enter' ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleSendShortcut('enter')}
                  >
                    Enter
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${sendShortcut === 'ctrl+enter' ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleSendShortcut('ctrl+enter')}
                  >
                    Ctrl+Enter
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${sendShortcut === 'shift+enter' ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleSendShortcut('shift+enter')}
                  >
                    Shift+Enter
                  </button>
                </div>
              </div>
            </>
          )}
        </div>

        <hr className={styles.sectionDivider} />

        {/* Behavior */}
        <div className={styles.aboutSection} id="sw-section-behavior">
          <div className={styles.themeLabel}>{t('software.behavior')}</div>
          {!loaded ? (
            <p className={styles.toolsEmpty}>{t('common.loading')}</p>
          ) : (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.autoName')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.autoNameDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    onClick={() => handleAutoTitle(true)}
                    className={`${styles.mcpTransportBtn} ${autoTitle ? styles.mcpTransportActive : ''}`}
                  >
                    {t('software.enable')}
                  </button>
                  <button
                    onClick={() => handleAutoTitle(false)}
                    className={`${styles.mcpTransportBtn} ${!autoTitle ? styles.mcpTransportActive : ''}`}
                  >
                    {t('software.disable')}
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.closeBehavior')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.closeBehaviorDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${closeToTray ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleCloseToTray(true)}
                  >
                    {t('software.minimizeToTray')}
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!closeToTray ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleCloseToTray(false)}
                  >
                    {t('software.quitApp')}
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.startToTray')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.startToTrayDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${startToTray ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleStartToTray(true)}
                  >
                    {t('software.silentToTray')}
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!startToTray ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleStartToTray(false)}
                  >
                    {t('software.showForeground')}
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.autoStart')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.autoStartDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${autoStart ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleAutoStart(true)}
                  >
                    {t('software.enable')}
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!autoStart ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleAutoStart(false)}
                  >
                    {t('software.disable')}
                  </button>
                </div>
              </div>
              {isWin32 && (
                <div className={styles.aboutRow}>
                  <div>
                    <span className={styles.aboutLabel}>{t('software.hwAccel')}</span>
                    <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>
                      {t('software.hwAccelDesc')}
                    </p>
                  </div>
                  <div className={styles.mcpTransportToggle}>
                    <button
                      className={`${styles.mcpTransportBtn} ${!disableHwAccel ? styles.mcpTransportActive : ''}`}
                      onClick={() => handleDisableHwAccel(false)}
                    >
                      {t('software.enable')}
                    </button>
                    <button
                      className={`${styles.mcpTransportBtn} ${disableHwAccel ? styles.mcpTransportActive : ''}`}
                      onClick={() => handleDisableHwAccel(true)}
                    >
                      {t('software.disable')}
                    </button>
                  </div>
                </div>
              )}
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.taskCompleteNotification')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.taskCompleteNotificationDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${taskCompleteNotification ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleTaskCompleteNotification(true)}
                  >
                    {t('software.enable')}
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!taskCompleteNotification ? styles.mcpTransportActive : ''}`}
                    onClick={() => handleTaskCompleteNotification(false)}
                  >
                    {t('software.disable')}
                  </button>
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.language')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.languageDesc')}</p>
                </div>
                <div style={{ width: 180 }}>
                  <Select
                    value={locale}
                    options={LOCALES.map(l => ({ value: l.code, label: l.label }))}
                    onChange={(v) => setLocale(v as Locale)}
                    variant="form"
                  />
                </div>
              </div>
              {/* Proxy */}
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.proxy')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.proxyDesc')}</p>
                </div>
                <div className={styles.mcpTransportToggle}>
                  <button
                    className={`${styles.mcpTransportBtn} ${useSystemProxy ? styles.mcpTransportActive : ''}`}
                    onClick={() => {
                      setUseSystemProxy(true)
                      loomRpc('config.set_tool_prefs', { http_proxy: '' }).then(() =>
                        useStore.getState().addToast({ type: 'success', message: t('software.proxySaved') })
                      ).catch(() =>
                        useStore.getState().addToast({ type: 'error', message: t('common.failed') })
                      )
                    }}
                  >
                    {t('software.enable')}
                  </button>
                  <button
                    className={`${styles.mcpTransportBtn} ${!useSystemProxy ? styles.mcpTransportActive : ''}`}
                    onClick={() => {
                      setUseSystemProxy(false)
                    }}
                  >
                    {t('software.disable')}
                  </button>
                </div>
              </div>
            </>
          )}
        </div>

        <hr className={styles.sectionDivider} />

        {/* Font */}
        <div className={styles.aboutSection} id="sw-section-font">
          <div className={styles.themeLabel}>{t('software.font')}</div>
          {!loaded ? (
            <p className={styles.toolsEmpty}>{t('common.loading')}</p>
          ) : (
            <>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.uiFont')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.uiFontDesc')}</p>
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
                  <span className={styles.aboutLabel}>{t('software.codeFont')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.codeFontDesc')}</p>
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
                <div className={styles.fontPreviewLabel}>{t('software.preview')}</div>
                <pre className={styles.fontPreviewCode} style={{ fontFamily: codeFont || undefined }}>
                  {`fn main() {\n  println!("Hello, openLoom");\n}`}
                </pre>
              </div>
            </>
          )}
        </div>

        <hr className={styles.sectionDivider} />

        {/* Appearance */}
        <div className={styles.aboutSection} id="sw-section-appearance">
          <div className={styles.themeLabel}>{t('software.appearance')}</div>

          {/* Theme */}
          <div className={styles.themeGrid}>
            {THEMES.map((th) => {
              const tc = th.id === 'custom' ? null : readThemeColors(th.id)
              const previewColors = th.id === 'custom'
                ? { bg: customColors.bg, surface: customColors.surface, accent: customColors.accent, text13: customColors.text + '22', text27: customColors.text + '44' }
                : tc
                  ? { bg: tc.bg, surface: tc.surface, accent: tc.accent, text13: tc.text13, text27: tc.text27 }
                  : { bg: '#000', surface: '#111', accent: '#666', text13: 'rgba(255,255,255,0.13)', text27: 'rgba(255,255,255,0.27)' }
              return (
              <button
                key={th.id}
                onClick={() => {
                  setTheme(th.id)
                  if (th.id === 'custom') applyCustomTheme(customColors)
                  useStore.getState().addToast({ type: 'success', message: _t('software.themeChanged', { theme: _t(`theme.${th.id}`) }) })
                }}
                className={`${styles.themeCard} ${theme === th.id ? styles.themeCardActive : ''}`}
              >
                <div
                  className={styles.themePreview}
                  style={{
                    '--pv-bg': previewColors.bg,
                    '--pv-surface': previewColors.surface,
                    '--pv-accent': previewColors.accent,
                    '--pv-text-13': previewColors.text13,
                    '--pv-text-27': previewColors.text27,
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
                <span className={`${styles.themeName} ${theme === th.id ? styles.themeNameActive : ''}`}>
                  {_t(`theme.${th.id}`)}
                </span>
              </button>
            )})}
          </div>

          {theme === 'custom' && (
            <div className={styles.customColors}>
              <div className={styles.customColorRow}>
                <label className={styles.customColorLabel}>
                  <span className={styles.customColorSwatch} style={{ background: customColors.bg }} />
                 {t('software.bgColor')}
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
                  {t('software.surfaceColor')}
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
                  {t('software.textColor')}
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
                  {t('software.accentColor')}
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
                  <span className={styles.aboutLabel}>{t('software.fontSize')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.fontSizeDesc')}</p>
                </div>
                <div style={{ width: 180 }}>
                  <Select
                    value={fontSize}
                    options={(Object.entries(FONT_SIZE_MAP) as [FontSizeId, { label: string; px: number }][]).map(([id, info]) => ({
                      value: id,
                      label: `${_t(`textSize.${id}`)} (${info.px}px)`,
                    }))}
                    onChange={(v) => setFontSize(v as FontSizeId)}
                    variant="form"
                  />
                </div>
              </div>
              <div className={styles.aboutRow}>
                <div>
                  <span className={styles.aboutLabel}>{t('software.resetToDefault')}</span>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('software.resetToDefaultDesc')}</p>
                </div>
                <button
                  onClick={handleResetSize}
                  className={styles.resetSizeBtn}
                >
                  {t('software.resetToDefault')}
                </button>
              </div>
            </>
          )}
        </div>

      </div>
    </>
  )
}
