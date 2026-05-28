import { StateCreator } from 'zustand'

export type ThemeId = 'dark' | 'light' | 'midnight' | 'warm-paper' | 'neon-pink' | 'ember'

export interface UiSlice {
  theme: ThemeId
  settingsOpen: boolean
  sidebarOpen: boolean
  setTheme: (theme: ThemeId) => void
  setSettingsOpen: (open: boolean) => void
  setSidebarOpen: (open: boolean) => void
  toggleSidebar: () => void
}

export const createUiSlice: StateCreator<UiSlice> = (set, get) => ({
  theme: 'dark',
  settingsOpen: false,
  sidebarOpen: true,
  setTheme: (theme) => {
    document.documentElement.setAttribute('data-theme', theme)
    window.hana.setPreference('theme', theme)
    set({ theme })
  },
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  toggleSidebar: () => set({ sidebarOpen: !get().sidebarOpen }),
})
