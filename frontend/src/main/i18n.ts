import { getStoreKey } from './store'

type Locale = 'zh-CN' | 'zh-TW' | 'en-US'

type TranslationMap = Record<string, string>

const zhCN: TranslationMap = {
  // ── Tray ─────────────────────────────────────────────────────────
  'tray.showLoom': '显示 openLoom',
  'tray.hidePet': '隐藏形象',
  'tray.showPet': '显示形象',
  'tray.dndOff': '关闭勿扰模式',
  'tray.dndOn': '开启勿扰模式',
  'tray.settings': '设置...',
  'tray.quit': '退出',
  // ── Pet context menu ─────────────────────────────────────────────
  'pet.sizeSmall': '大小：小 (128px)',
  'pet.sizeMedium': '大小：中 (192px)',
  'pet.sizeLarge': '大小：大 (256px)',
  'pet.closePet': '关闭形象',
  // ── Text context menu ────────────────────────────────────────────
  'menu.cut': '剪切',
  'menu.copy': '复制',
  'menu.paste': '粘贴',
  'menu.selectAll': '全选',
}

const enUS: TranslationMap = {
  // ── Tray ─────────────────────────────────────────────────────────
  'tray.showLoom': 'Show openLoom',
  'tray.hidePet': 'Hide Avatar',
  'tray.showPet': 'Show Avatar',
  'tray.dndOff': 'Disable DND',
  'tray.dndOn': 'Enable DND',
  'tray.settings': 'Settings...',
  'tray.quit': 'Quit',
  // ── Pet context menu ─────────────────────────────────────────────
  'pet.sizeSmall': 'Size: Small (128px)',
  'pet.sizeMedium': 'Size: Medium (192px)',
  'pet.sizeLarge': 'Size: Large (256px)',
  'pet.closePet': 'Close Avatar',
  // ── Text context menu ────────────────────────────────────────────
  'menu.cut': 'Cut',
  'menu.copy': 'Copy',
  'menu.paste': 'Paste',
  'menu.selectAll': 'Select All',
}

const zhTW: TranslationMap = {
  // ── Tray ─────────────────────────────────────────────────────────
  'tray.showLoom': '顯示 openLoom',
  'tray.hidePet': '隱藏形象',
  'tray.showPet': '顯示形象',
  'tray.dndOff': '關閉勿擾模式',
  'tray.dndOn': '開啟勿擾模式',
  'tray.settings': '設定...',
  'tray.quit': '退出',
  // ── Pet context menu ─────────────────────────────────────────────
  'pet.sizeSmall': '大小：小 (128px)',
  'pet.sizeMedium': '大小：中 (192px)',
  'pet.sizeLarge': '大小：大 (256px)',
  'pet.closePet': '關閉形象',
  // ── Text context menu ────────────────────────────────────────────
  'menu.cut': '剪下',
  'menu.copy': '複製',
  'menu.paste': '貼上',
  'menu.selectAll': '全選',
}

const packs: Record<Locale, TranslationMap> = {
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  'en-US': enUS,
}

function getLocale(): Locale {
  const stored = getStoreKey<string>('locale', 'zh-CN')
  if (stored === 'zh-CN' || stored === 'zh-TW' || stored === 'en-US') return stored
  return 'zh-CN'
}

export function t(key: string): string {
  const locale = getLocale()
  const pack = packs[locale]
  return pack[key] ?? key
}
