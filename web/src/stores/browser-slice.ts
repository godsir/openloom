export interface BrowserSlice { browserBySession: Record<string, unknown>; }
export function createBrowserSlice() { return { browserBySession: {} }; }
export function useAnyBrowserRunning() { return false; }
