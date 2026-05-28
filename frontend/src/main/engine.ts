import { spawn, ChildProcess } from 'child_process'
import { join } from 'path'
import { app } from 'electron'
import { existsSync } from 'fs'

let engineProcess: ChildProcess | null = null
let enginePort: number | null = null
let restartCount = 0
const MAX_RESTARTS = 5
const RESTART_DELAYS = [1000, 2000, 4000, 8000, 16000]
const START_TIMEOUT = 30000

function findProjectRoot(): string {
  let dir = process.cwd()
  for (let i = 0; i < 10; i++) {
    if (existsSync(join(dir, 'Cargo.toml'))) return dir
    dir = join(dir, '..')
  }
  return join(process.cwd(), '..')
}

function findLoomExe(): string {
  if (app.isPackaged) {
    return join(process.resourcesPath, 'engine', 'lume.exe')
  }
  const root = findProjectRoot()
  const release = join(root, 'target', 'release', 'lume.exe')
  if (existsSync(release)) return release
  const debug = join(root, 'target', 'debug', 'lume.exe')
  if (existsSync(debug)) return debug
  throw new Error(`lume.exe not found in ${root}/target/release or debug`)
}

export function getEnginePort(): number | null {
  return enginePort
}

export function startEngine(): Promise<number> {
  return new Promise((resolve, reject) => {
    let exePath: string
    try {
      exePath = findLoomExe()
    } catch (e: any) {
      reject(e)
      return
    }

    console.log(`[engine] starting: ${exePath}`)

    engineProcess = spawn(exePath, ['serve', '--port', '0'], {
      stdio: ['ignore', 'pipe', 'pipe'],
    })

    const timeout = setTimeout(() => {
      reject(new Error('Engine start timeout (30s)'))
    }, START_TIMEOUT)

    const tryPort = (line: string): number | null => {
      // JSON: {"type":"ready","port":56874}
      try {
        const obj = JSON.parse(line)
        if (obj.type === 'ready' && obj.port) return obj.port
      } catch { /* not JSON */ }
      // Text: http://HOST:PORT
      const m = line.match(/http:\/\/([\d.]+|\[::1\]):(\d+)/)
      return m ? parseInt(m[2], 10) : null
    }

    const onData = (data: Buffer) => {
      const text = data.toString()
      console.log('[engine]', text.trimEnd())
      for (const line of text.split('\n')) {
        const port = tryPort(line)
        if (port) {
          clearTimeout(timeout)
          enginePort = port
          restartCount = 0
          console.log(`[engine] ready on port ${port}`)
          resolve(port)
          return
        }
      }
    }

    engineProcess.stdout?.on('data', onData)
    engineProcess.stderr?.on('data', onData)

    engineProcess.on('exit', (code, signal) => {
      engineProcess = null
      enginePort = null
      const exitMsg = code != null ? `code=${code}` : `signal=${signal}`
      console.log(`[engine] exited (${exitMsg})`)
      if (restartCount < MAX_RESTARTS) {
        const delay = RESTART_DELAYS[restartCount]
        console.log(`[engine] restarting in ${delay}ms (attempt ${restartCount + 1}/${MAX_RESTARTS})`)
        restartCount++
        setTimeout(() => {
          startEngine().then((p) => { enginePort = p }).catch(console.error)
        }, delay)
      } else {
        console.error('[engine] crashed too many times, giving up')
      }
    })
  })
}

export async function stopEngine(): Promise<void> {
  return new Promise((resolve) => {
    if (!engineProcess) { resolve(); return }
    console.log('[engine] shutting down...')
    engineProcess.kill('SIGTERM')
    const forceKill = setTimeout(() => {
      console.log('[engine] force killing...')
      engineProcess?.kill('SIGKILL')
    }, 5000)
    engineProcess.on('exit', () => {
      clearTimeout(forceKill)
      engineProcess = null
      enginePort = null
      console.log('[engine] stopped')
      resolve()
    })
  })
}
