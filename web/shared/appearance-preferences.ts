/** Stub: appearance preferences (shared) */
export interface AppearancePreferences {
  theme?: string;
  serifFont?: string;
  paperTexture?: string;
}
export function isPaperTextureBlockedTheme(_theme: string | null): boolean { return false; }
export function isPaperTextureEnabled(_prefs: unknown): boolean { return true; }


