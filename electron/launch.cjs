#!/usr/bin/env node
/**
 * launch.cjs — openLoom 开发启动器
 *
 * 同时启动：
 *   1. Vite dev server（web/，HMR 热更新）
 *   2. Loom Rust 引擎 + Electron 窗口
 *
 * 等 Vite dev server 就绪后再启动 Electron，避免白屏。
 *
 * 用法：node launch.cjs [--dev] [--no-vite]
 */

const { spawn } = require('child_process');
const path = require('path');
const http = require('http');

// 必须在 spawn 前清除，否则 Electron 以 Node 模式运行
delete process.env.ELECTRON_RUN_AS_NODE;

const isDev = process.argv.includes('--dev') || true; // launch.cjs 仅用于 dev
const noVite = process.argv.includes('--no-vite');
const VITE_PORT = 5173;
const VITE_URL = `http://localhost:${VITE_PORT}`;

const electronExe = path.join(__dirname, 'node_modules', 'electron', 'dist', 'electron.exe');
const webDir = path.join(__dirname, '..', 'web');

// ── 检查 Vite dev server 是否已就绪 ──
function waitForVite(maxWaitMs = 30000) {
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + maxWaitMs;
    function check() {
      http.get(VITE_URL, (res) => {
        if (res.statusCode < 500) { resolve(); }
        else { retry(); }
        res.resume();
      }).on('error', () => {
        retry();
      });
    }
    function retry() {
      if (Date.now() > deadline) { reject(new Error('Vite dev server did not start in time')); return; }
      setTimeout(check, 300);
    }
    check();
  });
}

// ── 启动 Vite dev server ──
let viteProc = null;
function startVite() {
  if (noVite) return Promise.resolve();

  console.log('[launch] Starting Vite dev server...');
  // Windows: 用 cmd /c npm run dev 保证正确解析 npm.cmd
  const isWin = process.platform === 'win32';
  const [cmd, args] = isWin
    ? ['cmd', ['/c', 'npm', 'run', 'dev']]
    : ['npm', ['run', 'dev']];

  viteProc = spawn(cmd, args, {
    cwd: webDir,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env },
    shell: false,
  });

  viteProc.stdout.on('data', (d) => {
    const line = d.toString().trim();
    if (line) console.log('[vite]', line);
  });
  viteProc.stderr.on('data', (d) => {
    const line = d.toString().trim();
    if (line && !line.includes('ExperimentalWarning')) console.error('[vite]', line);
  });
  viteProc.on('exit', (code) => {
    if (code !== 0 && code !== null) console.error('[vite] exited with code', code);
  });

  return waitForVite(30000);
}

// ── 启动 Electron ──
function startElectron() {
  console.log('[launch] Starting Electron...');
  const child = spawn(electronExe, [__dirname, '--dev'], {
    stdio: 'inherit',
    env: {
      ...process.env,
      VITE_DEV_URL: VITE_URL,
    },
    windowsHide: false,
  });

  child.on('exit', (code) => {
    console.log('[launch] Electron exited with code', code);
    if (viteProc) viteProc.kill();
    process.exit(code ?? 0);
  });
}

// ── Main ──
(async () => {
  try {
    await startVite();
    console.log('[launch] Vite ready at', VITE_URL);
    startElectron();
  } catch (err) {
    console.error('[launch] Error:', err.message);
    if (viteProc) viteProc.kill();
    process.exit(1);
  }
})();
