const { execFileSync } = require('child_process')
const path = require('path')

// CI supplies ENGINE_BINARY explicitly. For local builds choose the native
// Rust executable so electron-builder's extraResources macro always resolves.
process.env.ENGINE_BINARY ||= process.platform === 'win32' ? 'loom.exe' : 'loom'

function run(command, args) {
  const packageJsonPath = require.resolve(`${command}/package.json`)
  const packageRoot = path.dirname(packageJsonPath)
  const packageJson = require(packageJsonPath)
  const bin = typeof packageJson.bin === 'string'
    ? packageJson.bin
    : packageJson.bin[command]
  execFileSync(process.execPath, [path.join(packageRoot, bin), ...args], {
    cwd: path.join(__dirname, '..'),
    env: process.env,
    stdio: 'inherit',
  })
}

run('electron-vite', ['build'])
const platformTarget = process.platform === 'win32' ? '--win'
  : process.platform === 'darwin' ? '--mac'
    : '--linux'
run('electron-builder', ['--publish', 'never', platformTarget])
