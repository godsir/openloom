import { getStoreKey } from './store'

type Locale = 'zh-CN' | 'en-US'

type TranslationMap = Record<string, string>

const zhCN: TranslationMap = {
  // ── Tray ─────────────────────────────────────────────────────────
  'tray.showLoom': '显示 openLoom',
  'tray.hidePet': '隐藏桌宠',
  'tray.showPet': '显示桌宠',
  'tray.dndOff': '关闭勿扰模式',
  'tray.dndOn': '开启勿扰模式',
  'tray.settings': '设置...',
  'tray.quit': '退出',
  // ── Pet context menu ─────────────────────────────────────────────
  'pet.sizeSmall': '大小：小 (128px)',
  'pet.sizeMedium': '大小：中 (192px)',
  'pet.sizeLarge': '大小：大 (256px)',
  'pet.closePet': '关闭桌宠',
}

const enUS: TranslationMap = {
  // ── Tray ─────────────────────────────────────────────────────────
  'tray.showLoom': 'Show openLoom',
  'tray.hidePet': 'Hide Pet',
  'tray.showPet': 'Show Pet',
  'tray.dndOff': 'Disable DND',
  'tray.dndOn': 'Enable DND',
  'tray.settings': 'Settings...',
  'tray.quit': 'Quit',
  // ── Pet context menu ─────────────────────────────────────────────
  'pet.sizeSmall': 'Size: Small (128px)',
  'pet.sizeMedium': 'Size: Medium (192px)',
  'pet.sizeLarge': 'Size: Large (256px)',
  'pet.closePet': 'Close Pet',
}

const packs: Record<Locale, TranslationMap> = {
  'zh-CN': zhCN,
  'en-US': enUS,
}

function getLocale(): Locale {
  const stored = getStoreKey<string>('locale', 'zh-CN')
  if (stored === 'zh-CN' || stored === 'en-US') return stored
  return 'zh-CN'
}

export function t(key: string): string {
  const locale = getLocale()
  const pack = packs[locale]
  return pack[key] ?? key
}
