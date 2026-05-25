const { app, BrowserWindow, Tray, Menu, session, nativeImage, ipcMain, dialog, shell } = require('electron');
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const os = require('os');
const https = require('https');
const http = require('http');

let mainWindow = null;
let engineProcess = null;
let enginePort = null;
let engineReady = false;
let retryCount = 0;
const MAX_RETRIES = 5;
const RETRY_DELAYS = [1000, 2000, 4000, 8000, 30000];
const READY_TIMEOUT_MS = 10000;

let tray = null;
let appIsQuitting = false;

function loadAppIcon() {
    const isDev = process.argv.includes('--dev');
    const iconName = isDev ? 'icon_dev.ico' : 'icon.ico';
    const iconPath = path.join(__dirname, 'icons', iconName);
    if (fs.existsSync(iconPath)) {
        return nativeImage.createFromPath(iconPath);
    }
    return nativeImage.createEmpty();
}

function startEngine() {
    const isDev = process.argv.includes('--dev');

    // Dev mode: engine is started externally by root npm run dev
    if (isDev) {
        const port = parseInt(process.env.LOOM_SERVER_PORT || '19876', 10);
        enginePort = port;
        engineReady = true;
        console.log(`Dev mode: using external engine on port ${enginePort}`);
        if (mainWindow) {
            mainWindow.webContents.executeJavaScript(
                `window.__enginePort__ = ${enginePort};`
            ).catch(() => {});
        }
        return;  // Don't spawn engine — managed externally
    }

    // Packaged: engine binary is in process.resourcesPath/engine/ (extraResources)
    // Dev: engine binary is in ../target/debug/ or ../target/release/
    const isPackaged = app.isPackaged;
    let engineExe;
    if (isPackaged) {
        engineExe = path.join(process.resourcesPath, 'engine', 'loom-server');
    } else {
        engineExe = path.join(__dirname, '..', 'target', 'release', 'loom-server');
    }
    const exePath = process.platform === 'win32' ? engineExe + '.exe' : engineExe;

    console.log(`Starting engine: ${exePath}`);
    engineProcess = spawn(exePath, ['serve', '--port', '0'], {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: { ...process.env, RUST_MIN_STACK: '8388608' },
    });

    // 10-second ready timeout
    const readyTimer = setTimeout(() => {
        if (!engineReady) {
            console.error('Engine failed to start within 10 seconds');
            if (mainWindow) {
                mainWindow.loadURL('data:text/html,<h1>Startup Failed</h1><p>Engine did not start. Check logs.</p>');
            }
        }
    }, READY_TIMEOUT_MS);

    engineProcess.stdout.on('data', (data) => {
        const lines = data.toString().trim().split('\n');
        for (const line of lines) {
            try {
                const msg = JSON.parse(line);
                if (msg.type === 'ready') {
                    enginePort = msg.port;
                    engineReady = true;
                    clearTimeout(readyTimer);
                    console.log(`Engine ready on port ${enginePort}`);
                    retryCount = 0;
                    // Inject port into renderer
                    if (mainWindow) {
                        mainWindow.webContents.executeJavaScript(
                            `window.__enginePort__ = ${enginePort};`
                        ).catch(() => {});
                    }
                }
            } catch (e) {
                // Non-JSON line (log output), ignore
            }
        }
    });

    engineProcess.stderr.on('data', (data) => {
        console.error(`Engine: ${data.toString().trim()}`);
    });

    engineProcess.on('exit', (code) => {
        engineReady = false;
        console.log(`Engine exited with code ${code}`);
        if (retryCount < MAX_RETRIES) {
            const delay = RETRY_DELAYS[retryCount] || 30000;
            console.log(`Restarting engine in ${delay}ms (attempt ${retryCount + 1}/${MAX_RETRIES})`);
            setTimeout(startEngine, delay);
            retryCount++;
        }
    });
}

function createWindow() {
    mainWindow = new BrowserWindow({
        width: 1200,
        height: 800,
        icon: loadAppIcon(),
        frame: false,
        titleBarStyle: 'hidden',
        autoHideMenuBar: true,
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: false,          // preload.js 用 require('electron')，不能沙盒化
            webviewTag: false,
        },
    });

    // Open DevTools in dev mode
    if (process.argv.includes('--dev')) {
        mainWindow.webContents.openDevTools({ mode: 'detach' });
    }

    // Inject engine port if already known
    if (enginePort) {
        mainWindow.webContents.on('did-finish-load', () => {
            mainWindow.webContents.executeJavaScript(
                `window.__enginePort__ = ${enginePort};`
            ).catch(() => {});
        });
    }

    const isDev = process.argv.includes('--dev');
    const viteDevUrl = process.env.VITE_DEV_URL || 'http://localhost:5173';

    if (isDev) {
        // Dev 模式：加载 Vite dev server（HMR 热更新）
        mainWindow.loadURL(viteDevUrl).catch((err) => {
            console.error('Failed to load Vite dev server:', err.message);
            console.error('Make sure to run: cd web && npm run dev');
        });
    } else {
        // 生产模式：加载 build 产物
        // Packaged: web/dist is in process.resourcesPath/web-dist/ (extraResources)
        const appRoot = app.isPackaged ? process.resourcesPath : path.join(__dirname, '..');
        const webDistDir = app.isPackaged ? 'web-dist' : path.join('web', 'dist');
        const reactDist = path.join(appRoot, webDistDir, 'index.html');
        mainWindow.loadFile(reactDist).catch(() => {
            mainWindow.loadURL(
                `data:text/html,<h1>openLoom</h1><p>Run "cd web && npm run build" for full UI</p>`
            );
        });
    }

    mainWindow.on('close', (event) => {
        if (!appIsQuitting) {
            event.preventDefault();
            mainWindow.hide();
        }
    });
}

function updateTrayMenu(statusLabel) {
    if (!tray) return;
    const contextMenu = Menu.buildFromTemplate([
        { label: '显示 openLoom', click: () => mainWindow?.show() },
        { type: 'separator' },
        { label: statusLabel, enabled: false },
        { type: 'separator' },
        { label: '退出', click: () => { appIsQuitting = true; app.quit(); }}
    ]);
    tray.setContextMenu(contextMenu);
}

function createTray() {
    const icon = loadAppIcon();
    tray = new Tray(icon.resize({ width: 16, height: 16 }));
    tray.setToolTip('openLoom');
    updateTrayMenu('Agent: Idle');
    tray.on('click', () => mainWindow?.show());
}

// ─── Search config helpers (preferences.json) ───────────────────

function getLoomDataDir() {
    const home = os.homedir();
    if (process.platform === 'win32') {
        return path.join(process.env.APPDATA || path.join(home, 'AppData', 'Roaming'), 'openLoom');
    } else if (process.platform === 'darwin') {
        return path.join(home, 'Library', 'Application Support', 'openLoom');
    } else {
        return path.join(process.env.XDG_DATA_HOME || path.join(home, '.local', 'share'), 'openLoom');
    }
}

function readPrefs() {
    try {
        const p = path.join(getLoomDataDir(), 'preferences.json');
        if (!fs.existsSync(p)) return {};
        return JSON.parse(fs.readFileSync(p, 'utf-8'));
    } catch { return {}; }
}

function writePrefs(prefs) {
    const p = path.join(getLoomDataDir(), 'preferences.json');
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, JSON.stringify(prefs, null, 2) + '\n');
}

function httpsFetch(url, options = {}) {
    const lib = url.startsWith('https') ? https : http;
    return new Promise((resolve, reject) => {
        const u = new URL(url);
        const req = lib.request({
            hostname: u.hostname,
            port: u.port,
            path: u.pathname + u.search,
            method: options.method || 'GET',
            headers: options.headers || {},
            timeout: 30000,
        }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                if (res.statusCode >= 400) {
                    reject(new Error(`HTTP ${res.statusCode}: ${data.slice(0, 200)}`));
                } else {
                    resolve(data);
                }
            });
        });
        req.on('error', reject);
        req.on('timeout', () => { req.destroy(); reject(new Error('Request timeout')); });
        if (options.body) req.write(options.body);
        req.end();
    });
}

async function verifyTavilyKey(key) {
    await httpsFetch('https://api.tavily.com/search', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: 'test', max_results: 1, api_key: key }),
    });
}

async function verifyBraveKey(key) {
    await httpsFetch('https://api.search.brave.com/res/v1/web/search?q=test&count=1', {
        headers: { 'X-Subscription-Token': key },
    });
}

async function verifySerperKey(key) {
    await httpsFetch('https://google.serper.dev/search', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-API-KEY': key },
        body: JSON.stringify({ q: 'test' }),
    });
}

// ─── IPC Handlers for desktop bridge (window.hana) ───────────────────
function registerIpcHandlers() {
    ipcMain.handle('get-platform', () => process.platform);

    ipcMain.handle('get-app-version', () => {
        try { return require('./package.json').version; } catch { return '0.1.0'; }
    });

    ipcMain.handle('select-folder', async () => {
        if (!mainWindow) return null;
        const result = await dialog.showOpenDialog(mainWindow, {
            properties: ['openDirectory'],
        });
        return result.canceled ? null : result.filePaths[0] || null;
    });

    ipcMain.handle('select-files', async () => {
        if (!mainWindow) return [];
        const result = await dialog.showOpenDialog(mainWindow, {
            properties: ['openFile', 'multiSelections'],
        });
        return result.canceled ? [] : result.filePaths;
    });

    ipcMain.handle('read-file', async (_event, filePath) => {
        try { return fs.readFileSync(filePath, 'utf-8'); } catch { return ''; }
    });

    ipcMain.handle('window-minimize', () => mainWindow?.minimize());
    ipcMain.handle('window-maximize', () => {
        if (mainWindow?.isMaximized()) mainWindow.unmaximize();
        else mainWindow?.maximize();
    });
    ipcMain.handle('window-close', () => mainWindow?.close());
    ipcMain.handle('window-is-maximized', () => mainWindow?.isMaximized() ?? false);

    mainWindow?.on('maximize', () => {
        mainWindow?.webContents.send('maximize-changed', true);
    });
    mainWindow?.on('unmaximize', () => {
        mainWindow?.webContents.send('maximize-changed', false);
    });

    ipcMain.on('start-drag', (_event, filePaths) => {
        if (!mainWindow) return;
        const { startDrag } = require('electron').app;
        if (startDrag) {
            // Electron 32+
            startDrag(filePaths);
        }
    });

    ipcMain.handle('app-ready', () => {
        console.log('Renderer signaled ready');
        return;
    });

    ipcMain.handle('open-external', async (_event, url) => {
        await shell.openExternal(url);
    });

    ipcMain.handle('open-folder', async (_event, dirPath) => {
        shell.openPath(dirPath);
    });

    ipcMain.handle('open-file', async (_event, filePath) => {
        shell.openPath(filePath);
    });

    // ── Search config (preferences.json) ──────────────────────────
    ipcMain.handle('search-config-get', () => {
        const prefs = readPrefs();
        return {
            provider: prefs.search_provider || 'auto',
            api_key: prefs.search_api_key || '',
            api_keys: prefs.search_api_keys || {},
        };
    });

    ipcMain.handle('search-config-set', (_event, partial) => {
        const prefs = readPrefs();
        if (partial.provider !== undefined) {
            if (partial.provider) prefs.search_provider = partial.provider;
            else delete prefs.search_provider;
        }
        if (partial.api_key !== undefined) {
            if (partial.api_key) prefs.search_api_key = partial.api_key;
            else delete prefs.search_api_key;
        }
        if (partial.api_keys !== undefined) {
            const keys = partial.api_keys;
            const existing = prefs.search_api_keys || {};
            for (const [k, v] of Object.entries(keys)) {
                if (v) existing[k] = v;
                else delete existing[k];
            }
            if (Object.keys(existing).length > 0) prefs.search_api_keys = existing;
            else delete prefs.search_api_keys;
        }
        writePrefs(prefs);
        return { ok: true };
    });

    ipcMain.handle('search-verify', async (_event, { provider, api_key }) => {
        try {
            const p = (provider || '').toLowerCase().trim();
            if (!p || p === 'auto') return { ok: true };
            if (p.endsWith('_browser')) return { ok: true };

            const key = String(api_key || '').trim();
            if (!key) return { ok: false, error: 'API key is required' };

            if (p === 'tavily') await verifyTavilyKey(key);
            else if (p === 'brave') await verifyBraveKey(key);
            else if (p === 'serper') await verifySerperKey(key);
            else if (p === 'anysearch') { /* free tier, no verification */ }
            else return { ok: false, error: `Unknown provider: ${provider}` };

            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
}

app.whenReady().then(() => {
    session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
        const isDev = process.argv.includes('--dev');
        callback({
            responseHeaders: {
                ...details.responseHeaders,
                'Content-Security-Policy': isDev
                    // Dev 模式：允许 localhost Vite dev server 的所有资源（HMR websocket、JS、CSS）
                    ? ["default-src 'self' 'unsafe-inline' 'unsafe-eval' http://localhost:* ws://localhost:* ws://127.0.0.1:* http://127.0.0.1:*; img-src 'self' data: file: http://localhost:* http://127.0.0.1:*; font-src 'self' data: http://localhost:*;"]
                    // 生产模式：严格 CSP
                    : ["default-src 'self'; connect-src ws://127.0.0.1:* http://127.0.0.1:* http://localhost:*; img-src 'self' data: file: http://127.0.0.1:*; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; font-src 'self' data:"]
            }
        });
    });

    // IPC for preload to query engine port (contextIsolation-safe).
    // Must be registered BEFORE createWindow — preload calls it on startup.
    ipcMain.handle('get-engine-port', () => enginePort);

    // Register desktop bridge IPC before window creation (safe: uses mainWindow?.)
    registerIpcHandlers();

    startEngine();
    setTimeout(createWindow, 2000);
    setTimeout(createTray, 3000);

    setTimeout(() => {
        setInterval(async () => {
            if (!enginePort) return;
            try {
                const http = require('http');
                const resp = await new Promise((resolve, reject) => {
                    http.get(`http://127.0.0.1:${enginePort}/health`, (res) => {
                        let data = '';
                        res.on('data', chunk => data += chunk);
                        res.on('end', () => resolve(JSON.parse(data)));
                    }).on('error', reject);
                });
                if (resp.status === 'degraded') {
                    updateTrayMenu('Agent: Degraded');
                    if (mainWindow) {
                        mainWindow.webContents.executeJavaScript(
                            `window.__engineStatus__ = 'degraded';`
                        ).catch(() => {});
                    }
                } else {
                    updateTrayMenu('Agent: Idle');
                }
            } catch {
                updateTrayMenu('Agent: Offline');
            }
        }, 30000);
    }, 5000);

    app.setLoginItemSettings({
        openAtLogin: false,
        path: app.getPath('exe'),
    });
});

app.on('before-quit', async () => {
    if (engineProcess && enginePort) {
        try {
            // Send graceful shutdown via JSON-RPC before killing
            await fetch(`http://127.0.0.1:${enginePort}/api`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    jsonrpc: '2.0',
                    method: 'system.shutdown',
                    params: {},
                    id: 1,
                }),
            });
        } catch (e) {
            // Engine may already be down
        }
        // Give engine time to drain and clean up
        setTimeout(() => {
            if (engineProcess && !engineProcess.killed) {
                engineProcess.kill('SIGKILL');
            }
        }, 5000);
    } else if (engineProcess) {
        engineProcess.kill('SIGTERM');
    }
});

app.on('window-all-closed', () => {
    app.quit();
});
