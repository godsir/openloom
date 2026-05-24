interface OpenLoomAPI {
    send: (method: string, params?: Record<string, unknown>) => Promise<Record<string, unknown>>;
    sseUrl: (sessionId: string) => string;
    subscribe: (event: string, callback: (data: Record<string, unknown>) => void) => () => void;
}

interface Window {
    openloom?: OpenLoomAPI;
    hana?: any;
    platform?: any;
    __enginePort__?: number;
    __engineStatus__?: string;
    i18n?: { t: (key: string, params?: Record<string, unknown>) => string; locale: string; load: (locale: string) => Promise<void> };
    t?: (key: string, params?: Record<string, unknown>) => string;
    __hanaLog?: (...args: unknown[]) => void;
    setTheme?: (id: string) => void;
    setSerifFont?: (v: string | boolean) => void;
    setPaperTexture?: (v: string | boolean) => void;
    loadSavedTheme?: () => void;
    loadSavedFont?: () => void;
    loadSavedPaperTexture?: () => void;
    initPlatform?: () => void;
}

declare var process: { env: Record<string, string | undefined> };

declare module '*.module.css' { const c: Record<string, string>; export default c; }
declare module '*.css' {}
declare module '*.png' { const v: string; export default v; }
declare module '*.jpg' { const v: string; export default v; }
declare module '*.svg' { const v: string; export default v; }
declare module 'markdown-it-task-lists' { const p: (md: unknown) => void; export default p; }
