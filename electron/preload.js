const { contextBridge } = require('electron');

contextBridge.exposeInMainWorld('openloom', {
    send: async (method, params) => {
        const ws = new WebSocket(`ws://127.0.0.1:${window.__enginePort__}/ws`);
        return new Promise((resolve, reject) => {
            ws.onopen = () => {
                ws.send(JSON.stringify({
                    jsonrpc: '2.0',
                    method,
                    params: params || {},
                    id: Date.now(),
                }));
            };
            ws.onmessage = (event) => {
                const data = JSON.parse(event.data);
                ws.close();
                if (data.error) {
                    reject(data.error);
                } else {
                    resolve(data.result);
                }
            };
            ws.onerror = (err) => reject(err);
        });
    },

    sseUrl: (sessionId) => {
        return `http://127.0.0.1:${window.__enginePort__}/sse/${sessionId}`;
    },

    subscribe: (eventType, callback) => {
        console.log('subscribe:', eventType);
    },
});
