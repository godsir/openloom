import { createContext, useContext, useState, useCallback, useEffect, type ReactNode } from 'react'
import type { Locale, TranslationMap } from './types'
import { zhCN } from './zh-CN'
import { zhTW } from './zh-TW'
import { enUS } from './en-US'

const packs: Record<Locale, TranslationMap> = {
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  'en-US': enUS,
}

function resolve(key: string, locale: Locale, vars?: Record<string, string | number>): string {
  const pack = packs[locale]
  if (!pack) {
    console.error(`[i18n] PACK MISSING for locale "${locale}". Available locales:`, Object.keys(packs))
    return key
  }
  const raw = pack[key]
  if (raw === undefined) {
    console.warn(`[i18n] missing key: "${key}" for locale "${locale}". Pack has ${Object.keys(pack).length} keys.`)
    return key
  }
  const text = typeof raw === 'function' ? raw({}) : raw
  if (vars) {
    return text.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`))
  }
  return text
}

interface LocaleCtx {
  locale: Locale
  setLocale: (l: Locale) => void
  t: (key: string, vars?: Record<string, string | number>) => string
}

const Ctx = createContext<LocaleCtx>({
  locale: 'zh-CN',
  setLocale: () => {},
  t: (k) => k,
})

export function LocaleProvider({ children, initial, onChange }: {
  children: ReactNode
  initial: Locale
  onChange?: (locale: Locale) => void
}) {
  const [locale, setLocaleState] = useState<Locale>(initial)

  const setLocale = useCallback((l: Locale) => {
    setLocaleState(l)
    onChange?.(l)
  }, [onChange])

  // Listen for backend-triggered locale changes (preferences.changed event)
  useEffect(() => {
    const handler = (e: Event) => {
      const lang = (e as CustomEvent).detail as Locale
      if (lang === 'zh-CN' || lang === 'zh-TW' || lang === 'en-US') {
        setLocaleState(lang)
        onChange?.(lang)
      }
    }
    window.addEventListener('loom-locale-changed', handler)
    return () => window.removeEventListener('loom-locale-changed', handler)
  }, [onChange])

  const t = useCallback(
    (key: string, vars?: Record<string, string | number>) => resolve(key, locale, vars),
    [locale],
  )

  return (
    <Ctx.Provider value={{ locale, setLocale, t }}>
      {children}
    </Ctx.Provider>
  )
}

export function useLocale() {
  return useContext(Ctx)
}

/**
 * Standalone t() function for use outside React components (e.g. service files).
 * Reads locale from localStorage (synced by LocaleProvider on change).
 */
export function t(key: string, vars?: Record<string, string | number>): string {
  let locale: Locale = 'zh-CN'
  try {
    const stored = localStorage.getItem('loom-locale')
    if (stored === 'zh-CN' || stored === 'zh-TW' || stored === 'en-US') locale = stored
  } catch { /* SSR / test env */ }
  return resolve(key, locale, vars)
}
