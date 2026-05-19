const { app, BrowserWindow } = require('electron');
const { spawn } = require('child_process');
const path = require('path');

let mainWindow = null;
let engineProcess = null;
let enginePort = null;
let retryCount = 0;
const MAX_RETRIES = 5;
const RETRY_DELAYS = [1000, 2000, 4000, 8000, 30000];

function startEngine() {
    const isDev = process.argv.includes('--dev');
    const engineExe = isDev
        ? path.join(__dirname, '..', 'target', 'debug', 'openloom')
        : path.join(__dirname, '..', 'target', 'release', 'openloom');

    // On Windows, append .exe
    const exePath = process.platform === 'win32' ? engineExe + '.exe' : engineExe;

    console.log(`Starting engine: ${exePath}`);
    engineProcess = spawn(exePath, ['serve', '--port', '0'], {
        stdio: ['pipe', 'pipe', 'pipe'],
    });

    engineProcess.stdout.on('data', (data) => {
        const lines = data.toString().trim().split('\n');
        for (const line of lines) {
            try {
                const msg = JSON.parse(line);
                if (msg.type === 'ready') {
                    enginePort = msg.port;
                    console.log(`Engine ready on port ${enginePort}`);
                    retryCount = 0;
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
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: true,
        },
    });

    // Load React build if available, otherwise placeholder
    const reactDist = path.join(__dirname, '..', 'web', 'dist', 'index.html');
    mainWindow.loadFile(reactDist).catch(() => {
        mainWindow.loadURL(
            `data:text/html,<h1>openLoom</h1><p>Engine port: ${enginePort || 'starting...'}</p><p>Run "cd web && npm run build" for full UI</p>`
        );
    });
}

app.whenReady().then(() => {
    startEngine();
    // Give engine time to start before creating window
    setTimeout(createWindow, 2000);
});

app.on('before-quit', () => {
    if (engineProcess) {
        engineProcess.kill('SIGTERM');
        setTimeout(() => {
            if (engineProcess && !engineProcess.killed) {
                engineProcess.kill('SIGKILL');
            }
        }, 5000);
    }
});

app.on('window-all-closed', () => {
    app.quit();
});
