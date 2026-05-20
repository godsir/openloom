const { contextBridge } = require('electron');

let ws = null;
let msgId = 0;
const pending = new Map();       // id → { resolve, reject, timer }
const subscribers = new Map();   // method → Set<callback>
let reconnectAttempt = 0;
let connectTime = null;
const MIN_UPTIME = 3000;
const MAX_BACKOFF = 30000;
const REQUEST_TIMEOUT = 30000;

function connect() {
    if (!window.__enginePort__) {
        setTimeout(connect, 500);
        return;
    }

    try {
        ws = new WebSocket(`ws://127.0.0.1:${window.__enginePort__}/ws`);
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
                const cbs = subscribers.get(data.method);
                if (cbs) {
                    cbs.forEach(cb => {
                        try { cb(data.params); } catch (e) { console.error('subscribe callback error:', e); }
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
        return `http://127.0.0.1:${window.__enginePort__}/sse/${sessionId}`;
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

// Initial connection attempt
if (window.__enginePort__) {
    connect();
} else {
    // Port not yet injected by main process — wait and retry
    const checkInterval = setInterval(() => {
        if (window.__enginePort__) {
            clearInterval(checkInterval);
            connect();
        }
    }, 200);
}
