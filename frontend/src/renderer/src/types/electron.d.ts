import type { LoomApi } from '../../preload/index'

declare global {
  interface Window {
    loom: LoomApi
    __enginePort__: number
    __isPackaged__: boolean
  }
}

export {}
