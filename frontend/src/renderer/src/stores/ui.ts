import { StateCreator } from 'zustand'

export type ThemeId = 'dark' | 'light' | 'midnight' | 'warm-paper' | 'neon-pink' | 'ember' | 'navy-gold' | 'umber-cream' | 'custom'

export interface CustomThemeColors {
  bg: string
  surface: string
  text: string
  accent: string
}
export type FontSizeId = 'small' | 'default' | 'large' | 'xlarge'

export const FONT_SIZE_MAP: Record<FontSizeId, { label: string; px: number }> = {
  small:   { label: '小 (13px)', px: 13 },
  default: { label: '默认 (14px)', px: 14 },
  large:   { label: '大 (15px)', px: 15 },
  xlarge:  { label: '超大 (16px)', px: 16 },
}

export interface UiSlice {
  theme: ThemeId
  fontSize: FontSizeId
  settingsOpen: boolean
  sidebarOpen: boolean
  permissionDrawerOpen: boolean
  thinkingDrawerOpen: boolean
  setTheme: (theme: ThemeId) => void
  setFontSize: (size: FontSizeId) => void
  setSettingsOpen: (open: boolean) => void
  setSidebarOpen: (open: boolean) => void
  toggleSidebar: () => void
  setPermissionDrawerOpen: (open: boolean) => void
  setThinkingDrawerOpen: (open: boolean) => void
}

function applyFontSize(size: FontSizeId) {
  // Use CSS zoom to scale the entire app proportionally (14px = zoom 1.0).
  // Applied only to .body (excludes the native titlebar).
  const px = FONT_SIZE_MAP[size].px
  const zoom = (px / 14).toFixed(3)
  document.documentElement.style.setProperty('--app-zoom', zoom)
}

export const createUiSlice: StateCreator<UiSlice> = (set, get) => ({
  theme: 'dark',
  fontSize: 'default',
  settingsOpen: false,
  sidebarOpen: true,
  permissionDrawerOpen: false,
  thinkingDrawerOpen: false,

  setTheme: (theme) => {
    document.documentElement.setAttribute('data-theme', theme)
    window.loom.setPreference('theme', theme)
    if (theme !== 'custom') {
      // Only clear theme-related inline CSS vars, preserve font/size settings
      const root = document.documentElement
      const vars = ['--bg','--bg-surface','--bg-card','--bg-active','--bg-overlay','--bg-input','--bg-tooltip',
        '--text','--text-secondary','--text-muted','--text-light',
        '--border','--border-default','--border-accent','--border-light',
        '--accent','--accent-hover','--accent-rgb','--accent-subtle','--accent-medium','--accent-glow','--accent-light','--accent-strong',
        '--shadow','--shadow-md','--shadow-lg','--shadow-glass']
      vars.forEach(v => root.style.removeProperty(v))
    }
    set({ theme })
  },

  setFontSize: (fontSize) => {
    applyFontSize(fontSize)
    window.loom.setPreference('fontSize', fontSize)
    set({ fontSize })
  },

  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  toggleSidebar: () => set({ sidebarOpen: !get().sidebarOpen }),
  setPermissionDrawerOpen: (open) => set({ permissionDrawerOpen: open }),
  setThinkingDrawerOpen: (open) => set({ thinkingDrawerOpen: open }),
})
