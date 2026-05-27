import { StateCreator } from 'zustand'

export type ThemeId = 'dark' | 'light' | 'midnight' | 'warm-paper'

export interface UiSlice {
  theme: ThemeId
  sidebarWidth: number
  activePanel: string | null
  settingsOpen: boolean
  setTheme: (theme: ThemeId) => void
  setSidebarWidth: (w: number) => void
  setActivePanel: (panel: string | null) => void
  setSettingsOpen: (open: boolean) => void
}

export const createUiSlice: StateCreator<UiSlice> = (set) => ({
  theme: 'dark',
  sidebarWidth: 280,
  activePanel: null,
  settingsOpen: false,
  setTheme: (theme) => {
    document.documentElement.setAttribute('data-theme', theme)
    set({ theme })
  },
  setSidebarWidth: (sidebarWidth) => set({ sidebarWidth }),
  setActivePanel: (activePanel) => set({ activePanel }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
})
