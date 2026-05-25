import registry from '../../shared/theme-registry';
import { hanaFetch } from '../hooks/use-hana-fetch';

export interface SyncedAppearancePreferences {
  theme?: string;
  serif?: boolean;
}

export function readBrowserAppearancePreferences(): Required<SyncedAppearancePreferences> {
  return {
    theme: registry.migrateSavedTheme(window.localStorage.getItem(registry.STORAGE_KEY)),
    serif: window.localStorage.getItem('hana-font-serif') !== '0',
  };
}

export function applySyncedAppearancePreferences(preferences?: SyncedAppearancePreferences | null): void {
  if (!preferences || typeof preferences !== 'object') return;
  if (preferences.theme) window.setTheme?.(preferences.theme);
  if (typeof preferences.serif === 'boolean') window.setSerifFont?.(preferences.serif);
}

export async function persistAppearancePreferences(
  preferences: SyncedAppearancePreferences = readBrowserAppearancePreferences(),
): Promise<void> {
  await hanaFetch('/api/preferences/appearance', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(preferences),
  });
}
