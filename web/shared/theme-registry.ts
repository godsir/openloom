/** Stub: theme registry (shared) */
interface ThemeDef { i18nName: string; i18nMode: string }
export const THEMES: Record<string, ThemeDef> = {
  'warm-paper': { i18nName: 'Warm Paper', i18nMode: 'Light' },
  'midnight': { i18nName: 'Midnight', i18nMode: 'Dark' },
  'high-contrast': { i18nName: 'High Contrast', i18nMode: 'Dark' },
  'grass-aroma': { i18nName: 'Grass Aroma', i18nMode: 'Light' },
  'contemplation': { i18nName: 'Contemplation', i18nMode: 'Dark' },
  'absolutely': { i18nName: 'Absolutely', i18nMode: 'Dark' },
  'delve': { i18nName: 'Delve', i18nMode: 'Light' },
  'deep-think': { i18nName: 'Deep Think', i18nMode: 'Dark' },
  'new-warm-paper': { i18nName: 'New Warm Paper', i18nMode: 'Light' },
};
export const AUTO_OPTION = { id: 'auto', label: 'Auto (system)', i18nName: 'Auto', i18nMode: 'Auto' };
export const STORAGE_KEY = 'theme';
export function getThemeIds(): string[] { return Object.keys(THEMES); }
export function migrateSavedTheme(_key: string | null): string | null { return null; }
export default { THEMES, AUTO_OPTION, STORAGE_KEY, getThemeIds, migrateSavedTheme };
