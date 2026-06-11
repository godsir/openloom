import { useState, useEffect } from 'react'
import { useLocale, type Locale } from '../../i18n'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useStore } from '../../stores'
import styles from './Onboarding.module.css'

// ── Types ──

type ProviderId = 'anthropic' | 'openai' | 'deepseek' | 'google' | 'groq' | 'zhipu' | 'moonshot' | 'qwen' | 'siliconflow' | 'doubao' | 'lmstudio' | 'ollama'

interface ProviderDef {
  id: ProviderId
  label: string
  backend: string
  defaultUrl: string
  apiFormat: 'openai' | 'anthropic'
}

const PROVIDERS: ProviderDef[] = [
  { id: 'anthropic', label: 'Anthropic', backend: 'Anthropic', defaultUrl: 'https://api.anthropic.com', apiFormat: 'anthropic' },
  { id: 'openai', label: 'OpenAI', backend: 'OpenAI', defaultUrl: 'https://api.openai.com/v1', apiFormat: 'openai' },
  { id: 'deepseek', label: 'DeepSeek', backend: 'DeepSeek', defaultUrl: 'https://api.deepseek.com/v1', apiFormat: 'openai' },
  { id: 'google', label: 'Google Gemini', backend: 'Custom', defaultUrl: 'https://generativelanguage.googleapis.com/v1beta/openai', apiFormat: 'openai' },
  { id: 'groq', label: 'Groq', backend: 'Custom', defaultUrl: 'https://api.groq.com/openai/v1', apiFormat: 'openai' },
  { id: 'zhipu', label: '智谱 GLM', backend: 'Custom', defaultUrl: 'https://open.bigmodel.cn/api/paas/v4', apiFormat: 'openai' },
  { id: 'moonshot', label: '月之暗面 Kimi', backend: 'Custom', defaultUrl: 'https://api.moonshot.cn/v1', apiFormat: 'openai' },
  { id: 'qwen', label: '通义千问', backend: 'Custom', defaultUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', apiFormat: 'openai' },
  { id: 'siliconflow', label: '硅基流动', backend: 'Custom', defaultUrl: 'https://api.siliconflow.cn/v1', apiFormat: 'openai' },
  { id: 'doubao', label: '豆包 ByteDance', backend: 'Custom', defaultUrl: 'https://ark.cn-beijing.volces.com/api/v3', apiFormat: 'openai' },
  { id: 'lmstudio', label: 'LM Studio', backend: 'LmStudio', defaultUrl: 'http://localhost:1234/v1', apiFormat: 'openai' },
  { id: 'ollama', label: 'Ollama', backend: 'Ollama', defaultUrl: 'http://localhost:11434/v1', apiFormat: 'openai' },
]

const LOCALE_OPTIONS: { value: Locale; label: string; native: string }[] = [
  { value: 'zh-CN', label: '中文简体', native: '中文简体' },
  { value: 'zh-TW', label: '中文繁體', native: '中文繁體' },
  { value: 'en-US', label: 'English', native: 'English' },
]

// ── Helpers ──

function normalizeBaseUrl(url: string, apiFormat: 'openai' | 'anthropic'): string {
  let u = url.trim().replace(/\/+$/, '')
  if (!u) return u
  if (apiFormat === 'openai' && !u.endsWith('/v1')) u = u + '/v1'
  return u
}

// ── Steps ──

const TOTAL_STEPS = 4

// ── Component ──

export default function Onboarding({ onComplete }: { onComplete: () => void }) {
  const { t, locale, setLocale } = useLocale()
  const setTheme = useStore((s) => s.setTheme)
  const theme = useStore((s) => s.theme)

  const [step, setStep] = useState(0)
  const [stepKey, setStepKey] = useState(0)

  // Step 0 – Language
  const [selLocale, setSelLocale] = useState<Locale>(locale)

  // Step 1 – Model
  const [selProvider, setSelProvider] = useState<ProviderDef>(PROVIDERS[0])
  const [apiKey, setApiKey] = useState('')
  const [savingModel, setSavingModel] = useState(false)

  // Step 2 – Workspace
  const [workDir, setWorkDir] = useState('')
  const [selTheme, setSelTheme] = useState(theme)

  // Load current workspace on mount
  useEffect(() => {
    loomRpc<{ workspace: string | null }>('workspace.get')
      .then((ws) => { if (ws.workspace) setWorkDir(ws.workspace) })
      .catch(() => {})
  }, [])

  // ── Navigation ──

  const goNext = () => {
    setStepKey((k) => k + 1)
    setStep((s) => Math.min(s + 1, TOTAL_STEPS - 1))
  }

  const goPrev = () => {
    setStepKey((k) => k + 1)
    setStep((s) => Math.max(s - 1, 0))
  }

  // ── Handlers ──

  const handleSelectLocale = (l: Locale) => {
    setSelLocale(l)
    setLocale(l)
  }

  const handleSaveModel = async () => {
    if (!apiKey.trim()) return
    setSavingModel(true)
    try {
      const result = await loomRpc<{ ok: boolean; env_name: string }>('model.save_key', {
        backend: selProvider.backend,
        api_key: apiKey.trim(),
        base_url: normalizeBaseUrl(selProvider.defaultUrl, selProvider.apiFormat),
        backend_label: selProvider.backend === 'Custom' ? selProvider.label : undefined,
      })
      if (result.ok) {
        useStore.getState().addToast({ type: 'success', message: t('modelPanel.apiKeySavedMsg', { env: result.env_name }) })
      }
    } catch { /* silent – user can configure later */ }
    setSavingModel(false)
  }

  const handlePickFolder = async () => {
    const path = await window.loom.selectFolder()
    if (path) {
      setWorkDir(path)
      await rpc('workspace.set_default', { path }, t('workspace.workspaceSet'))
    }
  }

  const handleSetTheme = (t: string) => {
    setSelTheme(t)
    setTheme(t as any)
  }

  const handleFinish = async () => {
    // Save model if key is entered
    if (apiKey.trim()) {
      try {
        await loomRpc('model.save_key', {
          backend: selProvider.backend,
          api_key: apiKey.trim(),
          base_url: normalizeBaseUrl(selProvider.defaultUrl, selProvider.apiFormat),
        })
      } catch { /* ignore */ }
    }
    window.loom.setPreference('onboarded', true)
    onComplete()
  }

  // ── Render helpers ──

  const stepLabel = (s: number) => {
    const map: Record<number, string> = {
      0: t('onboarding.stepLanguage'),
      1: t('onboarding.stepModel'),
      2: t('onboarding.stepWorkspace'),
      3: t('onboarding.stepReady'),
    }
    return map[s] ?? ''
  }

  return (
    <div className={styles.backdrop}>
      {/* BG orbs */}
      <div className={`${styles.bgOrb} ${styles.bgOrb1}`} />
      <div className={`${styles.bgOrb} ${styles.bgOrb2}`} />
      <div className={`${styles.bgOrb} ${styles.bgOrb3}`} />
      <div className={styles.bgGrid} />

      <div className={styles.card}>
        {/* ── Top: title + horizontal step indicator ── */}
        <div className={styles.top}>
          <h1 className={styles.appTitle}>{t('onboarding.welcomeTitle')}</h1>

          <div className={styles.steps}>
            {Array.from({ length: TOTAL_STEPS }).map((_, i) => (
              <div key={i} className={styles.stepRow}>
                <div
                  className={`${styles.stepDot} ${
                    i < step ? styles.stepDone : i === step ? styles.stepActive : styles.stepPending
                  }`}
                >
                  {i < step ? (
                    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
                      <polyline points="20 6 9 17 4 12" />
                    </svg>
                  ) : (
                    i + 1
                  )}
                </div>
                <span
                  className={`${styles.stepLabel} ${
                    i <= step ? styles.stepLabelActive : styles.stepLabelPending
                  }`}
                >
                  {stepLabel(i)}
                </span>
              </div>
            ))}
          </div>

          <div className={styles.stepTrack}>
            <div
              className={styles.stepTrackFill}
              style={{ width: `${((step) / (TOTAL_STEPS - 1)) * 100}%` }}
            />
          </div>
        </div>

        {/* ── Content area ── */}
        <div key={stepKey} className={styles.content}>
          {/* ═══ Step 0: Language ═══ */}
          {step === 0 && (
            <>
              <h2 className={styles.title}>{t('onboarding.selectLanguage')}</h2>
              <p className={styles.desc}>{t('onboarding.languageDesc')}</p>
              <div className={styles.localeGrid}>
                {LOCALE_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={`${styles.localeCard} ${selLocale === opt.value ? styles.localeCardActive : ''}`}
                    onClick={() => handleSelectLocale(opt.value)}
                  >
                    <span className={styles.localeFlag}>
                      {opt.value === 'zh-CN' ? '🇨🇳' : opt.value === 'zh-TW' ? '🇹🇼' : '🇺🇸'}
                    </span>
                    <span className={styles.localeName}>{opt.native}</span>
                    <span className={styles.localeCode}>{opt.value}</span>
                  </button>
                ))}
              </div>
            </>
          )}

          {/* ═══ Step 1: Model ═══ */}
          {step === 1 && (
            <>
              <h2 className={styles.title}>{t('onboarding.selectModel')}</h2>
              <p className={styles.desc}>{t('onboarding.modelDesc')}</p>

              <div className={styles.providerGrid}>
                {PROVIDERS.map((p) => (
                  <button
                    key={p.id}
                    className={`${styles.providerCard} ${selProvider.id === p.id ? styles.providerCardActive : ''}`}
                    onClick={() => setSelProvider(p)}
                  >
                    {p.label}
                    {p.id === 'lmstudio' || p.id === 'ollama' ? (
                      <span className={styles.localBadge}>{t('onboarding.localBadge')}</span>
                    ) : null}
                  </button>
                ))}
              </div>

              <div className={styles.apiKeyRow}>
                <input
                  type="password"
                  className={styles.apiKeyInput}
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder={t('onboarding.apiKeyPlaceholder', { provider: selProvider.label })}
                />
              </div>
            </>
          )}

          {/* ═══ Step 2: Workspace ═══ */}
          {step === 2 && (
            <>
              <h2 className={styles.title}>{t('onboarding.setupWorkspace')}</h2>
              <p className={styles.desc}>{t('onboarding.workspaceDesc')}</p>

              <div className={styles.workDirRow}>
                <div className={styles.workDirInfo}>
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
                  </svg>
                  <span className={styles.workDirPath}>
                    {workDir || t('onboarding.noFolderSelected')}
                  </span>
                </div>
                <button className={styles.browseBtn} onClick={handlePickFolder}>
                  {t('onboarding.browseFolder')}
                </button>
              </div>

              <div className={styles.themeSection}>
                <span className={styles.themeLabel}>{t('onboarding.themeLabel')}</span>
                <div className={styles.themeGrid}>
                  {[
                    { id: 'dark', label: t('theme.dark'), cls: styles.themeDark },
                    { id: 'midnight', label: t('theme.midnight'), cls: styles.themeMidnight },
                    { id: 'light', label: t('theme.light'), cls: styles.themeLight },
                    { id: 'warm-paper', label: t('theme.warm-paper'), cls: styles.themeWarm },
                  ].map((th) => (
                    <button
                      key={th.id}
                      className={`${styles.themeCard} ${th.cls} ${selTheme === th.id ? styles.themeCardActive : ''}`}
                      onClick={() => handleSetTheme(th.id)}
                    >
                      <span className={styles.themeSwatch} />
                      <span className={styles.themeName}>{th.label}</span>
                    </button>
                  ))}
                </div>
              </div>
            </>
          )}

          {/* ═══ Step 3: Ready ═══ */}
          {step === 3 && (
            <>
              <div className={styles.readyCheck}>
                <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                  <polyline points="22 4 12 14.01 9 11.01" />
                </svg>
              </div>
              <h2 className={styles.title}>{t('onboarding.readyTitle')}</h2>
              <p className={styles.desc}>{t('onboarding.readyDesc')}</p>
              <div className={styles.readySummary}>
                <div className={styles.summaryItem}>
                  <span className={styles.summaryKey}>{t('onboarding.stepLanguage')}</span>
                  <span className={styles.summaryVal}>
                    {LOCALE_OPTIONS.find((l) => l.value === selLocale)?.native}
                  </span>
                </div>
                <div className={styles.summaryItem}>
                  <span className={styles.summaryKey}>{t('onboarding.stepModel')}</span>
                  <span className={styles.summaryVal}>
                    {apiKey.trim() ? selProvider.label : t('onboarding.skipped')}
                  </span>
                </div>
                <div className={styles.summaryItem}>
                  <span className={styles.summaryKey}>{t('onboarding.stepWorkspace')}</span>
                  <span className={styles.summaryVal}>
                    {workDir ? workDir.split(/[/\\]/).pop() : t('onboarding.defaultFolder')}
                  </span>
                </div>
              </div>
            </>
          )}
        </div>

        {/* ── Actions ── */}
        <div className={styles.actions}>
          {step > 0 && (
            <button onClick={goPrev} className={styles.btnSecondary}>
              {t('onboarding.prev')}
            </button>
          )}
          {step === 1 && (
            <button onClick={goNext} className={styles.btnSkip}>
              {t('onboarding.skipModel')}
            </button>
          )}
          {step < TOTAL_STEPS - 1 ? (
            <button
              onClick={() => {
                if (step === 1 && apiKey.trim()) handleSaveModel()
                goNext()
              }}
              className={styles.btnPrimary}
            >
              {t('onboarding.next')}
            </button>
          ) : (
            <button onClick={handleFinish} className={styles.btnPrimary}>
              {t('onboarding.startUsing')}
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
