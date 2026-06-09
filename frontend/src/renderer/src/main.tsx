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
