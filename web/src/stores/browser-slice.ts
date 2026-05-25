import { useStore } from './index';

export interface BrowserSlice { browserBySession: Record<string, unknown>; }
export function createBrowserSlice() { return { browserBySession: {} }; }
export function useAnyBrowserRunning() { return false; }

export function useBrowserState() {
  return useStore(s => ({
    running: false,
    url: null as string | null,
    thumbnail: null as string | null,
  }));
}
