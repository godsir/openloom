import type { HanaApi } from '../../preload/index'

declare global {
  interface Window {
    hana: HanaApi
    __enginePort__: number
  }
}

export {}
