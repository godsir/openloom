import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { LocaleProvider } from './i18n'
import type { Locale } from './i18n'
import './services/pet-commands'
import './styles/base.css'
import './styles/fonts.css'
import './themes/light.css'
import './themes/midnight.css'
import './themes/warm-paper.css'
import './themes/neon-pink.css'
import './themes/ember.css'
import './themes/navy-gold.css'
import './themes/umber-cream.css'

// ── Polyfill: Map.prototype.getOrInsertComputed (ES2024) ──
// pdfjs-dist v6 calls this method internally (e.g. `this.#objs.getOrInsertComputed(...)`).
// Vite/esbuild's private-field transform rewrites the call to
// `privateGet(this, _objs).getOrInsertComputed(...)`, which fails at runtime when
// the V8 version shipped with Electron 38 hasn't yet implemented the spec method.
// Spec: https://github.com/tc39/proposal-upsert
if (typeof Map !== 'undefined' && typeof (Map.prototype as any).getOrInsertComputed !== 'function') {
  ;(Map.prototype as any).getOrInsertComputed = function getOrInsertComputed(
    this: Map<unknown, unknown>,
    key: unknown,
    callback: (key: unknown) => unknown,
  ): unknown {
    if (this.has(key)) return this.get(key)
    const value = callback(key)
    this.set(key, value)
    return value
  }
}

function getInitialLocale(): Locale {
  // localStorage for renderer persistence; window.loom preference as backend-synced source
  const stored = localStorage.getItem('loom-locale')
  if (stored === 'zh-CN' || stored === 'zh-TW' || stored === 'en-US') return stored
  return 'zh-CN'
}

function handleLocaleChange(locale: Locale) {
  localStorage.setItem('loom-locale', locale)
  try { window.loom?.setPreference?.('locale', locale) } catch { /* preload not available */ }
  document.documentElement.lang = locale
}

const initialLocale = getInitialLocale()
document.documentElement.lang = initialLocale

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <LocaleProvider initial={initialLocale} onChange={handleLocaleChange}>
      <App />
    </LocaleProvider>
  </StrictMode>,
)
