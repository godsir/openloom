declare module '@hana/plugin-protocol' {
  export const PLUGIN_UI_CAPABILITY: {
    UI_RESIZE: string;
    [key: string]: string;
  };
  export const PLUGIN_UI_ERROR_CODE: Record<string, string>;
  export const PLUGIN_UI_PROTOCOL: string;
  export const PLUGIN_UI_PROTOCOL_VERSION: number;

  export function parsePluginUiMessage(data: unknown): { ok: boolean; value: PluginUiMessage };

  export interface PluginUiMessage {
    protocol?: string;
    version?: number;
    id: string;
    kind: string;
    type: string;
    payload?: unknown;
    responseOf?: string;
    error?: { code: string; message: string; details?: unknown };
  }

  export type PluginUiCapability = string;
}
