const { contextBridge, ipcRenderer, webUtils } = require('electron');

let ws = null;
let msgId = 0;
const pending = new Map();       // id → { resolve, reject, timer }
const subscribers = new Map();   // method → Set<callback>
let reconnectAttempt = 0;
let connectTime = null;
const MIN_UPTIME = 3000;
const MAX_BACKOFF = 30000;
const REQUEST_TIMEOUT = 30000;

// Port is fetched from main process via IPC (contextIsolation-safe)
let enginePort = 0;

function connect() {
    if (!enginePort) {
        setTimeout(connect, 500);
        return;
    }

    try {
        ws = new WebSocket(`ws://127.0.0.1:${enginePort}/ws`);
    } catch (e) {
        scheduleReconnect();
        return;
    }

    ws.onopen = () => {
        reconnectAttempt = 0;
        connectTime = Date.now();
    };

    ws.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);
            if (data.id !== undefined) {
                // JSON-RPC response
                const entry = pending.get(data.id);
                if (entry) {
                    clearTimeout(entry.timer);
                    pending.delete(data.id);
                    if (data.error) {
                        entry.reject(data.error);
                    } else {
                        entry.resolve(data.result);
                    }
                }
            } else {
                // JSON-RPC notification (no id field)
                // Dispatch to exact-method subscribers
                const cbs = subscribers.get(data.method);
                if (cbs) {
                    cbs.forEach(cb => {
                        try { cb(data); } catch (e) { console.error('subscribe callback error:', e); }
                    });
                }
                // Dispatch to wildcard '*' subscribers (full message object)
                const wildcardCbs = subscribers.get('*');
                if (wildcardCbs) {
                    wildcardCbs.forEach(cb => {
                        try { cb(data); } catch (e) { console.error('wildcard subscribe callback error:', e); }
                    });
                }
            }
        } catch (e) {
            console.error('ws message parse error:', e);
        }
    };

    ws.onerror = () => {
        // onclose will fire after onerror, triggering reconnect
    };

    ws.onclose = () => {
        // Reject all pending requests
        pending.forEach((entry) => {
            clearTimeout(entry.timer);
            entry.reject(new Error('WebSocket disconnected'));
        });
        pending.clear();

        // Fast reconnect if connection was unstable
        if (connectTime && (Date.now() - connectTime) < MIN_UPTIME) {
            reconnectAttempt += 2;
        }

        scheduleReconnect();
    };
}

function scheduleReconnect() {
    const jitter = Math.floor(Math.random() * 1000);
    const delay = Math.min(1000 * Math.pow(1.5, reconnectAttempt) + jitter, MAX_BACKOFF);
    reconnectAttempt++;
    setTimeout(connect, delay);
}

function ensureConnection() {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
        return new Promise((resolve, reject) => {
            const check = setInterval(() => {
                if (ws && ws.readyState === WebSocket.OPEN) {
                    clearInterval(check);
                    resolve();
                }
            }, 100);
            setTimeout(() => {
                clearInterval(check);
                reject(new Error('Connection timeout'));
            }, 10000);
        });
    }
    return Promise.resolve();
}

contextBridge.exposeInMainWorld('openloom', {
    send: async (method, params) => {
        await ensureConnection();
        const id = ++msgId;
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                pending.delete(id);
                reject(new Error(`Request timeout: ${method}`));
            }, REQUEST_TIMEOUT);
            pending.set(id, { resolve, reject, timer });
            ws.send(JSON.stringify({ jsonrpc: '2.0', method, params: params || {}, id }));
        });
    },

    sseUrl: (sessionId) => {
        return `http://127.0.0.1:${enginePort}/sse/${sessionId}`;
    },

    subscribe: (eventType, callback) => {
        if (!subscribers.has(eventType)) {
            subscribers.set(eventType, new Set());
        }
        subscribers.get(eventType).add(callback);

        // Ensure WebSocket is connected
        if (!ws || ws.readyState !== WebSocket.OPEN) {
            connect();
        }

        // Return unsubscribe function
        return () => {
            const cbs = subscribers.get(eventType);
            if (cbs) {
                cbs.delete(callback);
                if (cbs.size === 0) {
                    subscribers.delete(eventType);
                }
            }
        };
    },
});

// ─── Desktop IPC bridge (window.hana) ───────────────────────────────
contextBridge.exposeInMainWorld('hana', {
    getServerPort: () => ipcRenderer.invoke('get-engine-port').then(p => String(p || '')),
    getServerToken: () => ipcRenderer.invoke('get-engine-token').then(t => String(t || '')),

    getFilePath: (file) => {
        try { return webUtils.getPathForFile(file); } catch { return ''; }
    },

    getFileUrl: (p) => {
        if (!p) return '';
        try { return require('url').pathToFileURL(p).href; } catch { return 'file:///' + p.replace(/\\/g, '/'); }
    },

    getSplashInfo: () => ipcRenderer.invoke('get-splash-info'),
    onboardingComplete: () => ipcRenderer.invoke('onboarding-complete'),
    reloadMainWindow: () => ipcRenderer.invoke('reload-main-window'),

    selectFolder: () => ipcRenderer.invoke('select-folder'),
    selectFiles: () => ipcRenderer.invoke('select-files'),
    readFile: (p) => ipcRenderer.invoke('read-file', p),

    getPlatform: () => ipcRenderer.invoke('get-platform'),

    windowMinimize:    () => ipcRenderer.invoke('window-minimize'),
    windowMaximize:    () => ipcRenderer.invoke('window-maximize'),
    windowClose:       () => ipcRenderer.invoke('window-close'),
    windowIsMaximized: () => ipcRenderer.invoke('window-is-maximized'),

    onMaximizeChange: (cb) => {
        const handler = (_event, max) => cb(max);
        ipcRenderer.on('maximize-changed', handler);
        return () => ipcRenderer.removeListener('maximize-changed', handler);
    },

    getAppVersion: () => ipcRenderer.invoke('get-app-version'),
    startDrag: (paths) => ipcRenderer.send('start-drag', paths),
    appReady: () => ipcRenderer.invoke('app-ready'),
    openExternal: (url) => ipcRenderer.invoke('open-external', url),
    openFolder: (path) => ipcRenderer.invoke('open-folder', path),
    openFile: (path) => ipcRenderer.invoke('open-file', path),

    // Search engine config
    getSearchConfig: () => ipcRenderer.invoke('search-config-get'),
    setSearchConfig: (partial) => ipcRenderer.invoke('search-config-set', partial),
    searchVerify: (params) => ipcRenderer.invoke('search-verify', params),
});

// ─── Port acquisition via IPC (contextIsolation-safe) ──────────────
// Under contextIsolation, the preload's window is isolated from the
// renderer's window. executeJavaScript() writes to the renderer's
// window, so the preload can't read window.__enginePort__ directly.
// Instead we fetch the port from the main process via IPC.

async function acquirePort() {
    while (true) {
        try {
            const port = await ipcRenderer.invoke('get-engine-port');
            if (port && port > 0) {
                enginePort = port;
                // Also expose to renderer world so React's app-init.ts can read it
                // We do this by injecting it via a contextBridge-exposed variable.
                console.log('[preload] Engine port acquired via IPC:', port);
                connect();
                return;
            }
        } catch (e) {
            // Main process may not have handler registered yet
        }
        await new Promise(r => setTimeout(r, 300));
    }
}

acquirePort();
