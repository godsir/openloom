export type Locale = 'zh-CN' | 'zh-TW' | 'en-US'

export interface LocaleMeta {
  code: Locale
  label: string
}

export const LOCALES: LocaleMeta[] = [
  { code: 'zh-CN', label: '简体中文' },
  { code: 'zh-TW', label: '繁體中文' },
  { code: 'en-US', label: 'English' },
]

export type TranslationValue = string | ((vars: Record<string, string | number>) => string)
export type TranslationMap = Record<string, TranslationValue>
