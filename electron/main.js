const { app, BrowserWindow, Tray, Menu, session, nativeImage, ipcMain, dialog, shell } = require('electron');
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

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
    const engineExe = isDev
        ? path.join(__dirname, '..', 'target', 'debug', 'loom-server')
        : path.join(__dirname, '..', 'target', 'release', 'loom-server');

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
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: true,
            webviewTag: false,
        },
    });

    // Inject engine port if already known
    if (enginePort) {
        mainWindow.webContents.on('did-finish-load', () => {
            mainWindow.webContents.executeJavaScript(
                `window.__enginePort__ = ${enginePort};`
            ).catch(() => {});
        });
    }

    const reactDist = path.join(__dirname, '..', 'web', 'dist', 'index.html');
    mainWindow.loadFile(reactDist).catch(() => {
        mainWindow.loadURL(
            `data:text/html,<h1>openLoom</h1><p>Engine port: ${enginePort || 'starting...'}</p><p>Run "cd web && npm run build" for full UI</p>`
        );
    });

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

// ─── IPC Handlers for desktop bridge (window.hana) ───────────────────
function registerIpcHandlers() {
    ipcMain.handle('get-engine-port', () => enginePort || 0);

    ipcMain.handle('get-engine-token', () => '');

    ipcMain.handle('get-splash-info', () => ({}));

    ipcMain.handle('onboarding-complete', () => {
        console.log('Onboarding completed');
        return;
    });

    ipcMain.handle('reload-main-window', () => {
        if (mainWindow) mainWindow.reload();
    });

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
}

app.whenReady().then(() => {
    session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
        callback({
            responseHeaders: {
                ...details.responseHeaders,
                'Content-Security-Policy': [
                    "default-src 'self'; connect-src ws://127.0.0.1:* http://127.0.0.1:* http://localhost:*; img-src 'self' data: file: http://127.0.0.1:*; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; font-src 'self' data:"
                ]
            }
        });
    });

    startEngine();
    registerIpcHandlers();
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
