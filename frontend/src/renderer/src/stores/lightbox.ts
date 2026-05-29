import { StateCreator } from 'zustand'

export interface LightboxState {
  lightboxSrc: string | null
}

export interface LightboxSlice {
  lightbox: LightboxState
  openLightbox: (src: string) => void
  closeLightbox: () => void
}

export const createLightboxSlice: StateCreator<LightboxSlice> = (set) => ({
  lightbox: { lightboxSrc: null },

  openLightbox: (src: string) => {
    set({ lightbox: { lightboxSrc: src } })
  },

  closeLightbox: () => {
    set({ lightbox: { lightboxSrc: null } })
  },
})
