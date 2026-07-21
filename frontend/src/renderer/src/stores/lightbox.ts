import { StateCreator } from 'zustand'

export interface LightboxState {
  lightboxSrc: string | null
  /** 当前图片组（支持 ←/→ 切换）；单图打开时仅含该图 */
  lightboxList: string[]
  lightboxIndex: number
}

export interface LightboxSlice {
  lightbox: LightboxState
  openLightbox: (src: string, list?: string[], index?: number) => void
  closeLightbox: () => void
  nextLightboxImage: () => void
  prevLightboxImage: () => void
}

export const createLightboxSlice: StateCreator<LightboxSlice> = (set, get) => ({
  lightbox: { lightboxSrc: null, lightboxList: [], lightboxIndex: 0 },

  openLightbox: (src, list, index) => {
    set({
      lightbox: {
        lightboxSrc: src,
        lightboxList: list && list.length > 0 ? list : [src],
        lightboxIndex: index ?? 0,
      },
    })
  },

  closeLightbox: () => {
    set({ lightbox: { lightboxSrc: null, lightboxList: [], lightboxIndex: 0 } })
  },

  nextLightboxImage: () => {
    const { lightboxList, lightboxIndex } = get().lightbox
    if (lightboxList.length <= 1) return
    const next = (lightboxIndex + 1) % lightboxList.length
    set({ lightbox: { lightboxSrc: lightboxList[next], lightboxList, lightboxIndex: next } })
  },

  prevLightboxImage: () => {
    const { lightboxList, lightboxIndex } = get().lightbox
    if (lightboxList.length <= 1) return
    const prev = (lightboxIndex - 1 + lightboxList.length) % lightboxList.length
    set({ lightbox: { lightboxSrc: lightboxList[prev], lightboxList, lightboxIndex: prev } })
  },
})
