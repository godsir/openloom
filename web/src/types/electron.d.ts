export interface OpenLoomAPI {
    send: (method: string, params?: Record<string, unknown>) => Promise<Record<string, unknown>>;
    sseUrl: (sessionId: string) => string;
    subscribe: (event: string, callback: (data: Record<string, unknown>) => void) => () => void;
}

declare global {
    interface Window {
        openloom?: OpenLoomAPI;
        __enginePort__?: number;
        __engineStatus__?: string;
    }
}

export {};
