import { useState, useEffect, useRef, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import settingsStyles from './SettingsModal.module.css'
import styles from './WorkspaceTab.module.css'

interface SandboxConfig {
  enabled: boolean
  workspace_only: boolean
  allowed_paths: string[]
  denied_paths: string[]
}

export default function WorkspaceTab() {
  const [defaultPath, setDefaultPath] = useState('')
  const [loading, setLoading] = useState(true)
  const [sandbox, setSandbox] = useState<SandboxConfig>({ enabled: false, workspace_only: true, allowed_paths: [], denied_paths: [] })
  const sandboxRef = useRef(sandbox)
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const sessions = useStore(s => s.sessions)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)

  useEffect(() => {
    Promise.all([
      loomRpc<{ workspace: string | null }>('workspace.get'),
      loomRpc<SandboxConfig>('config.get_sandbox'),
    ]).then(([ws, sb]) => {
      setDefaultPath(ws.workspace || '')
      setSandbox(sb)
      sandboxRef.current = sb
      setLoading(false)
    }).catch(() => setLoading(false))
  }, [])

  // Auto-save sandbox on change (debounced for text inputs)
  const saveSandbox = useCallback((next: SandboxConfig) => {
    sandboxRef.current = next
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      loomRpc('config.set_sandbox', next as unknown as Record<string, unknown>).catch(() => {})
    }, 400)
  }, [])

  const updateSandbox = useCallback((patch: Partial<SandboxConfig>) => {
    setSandbox(prev => {
      const next = { ...prev, ...patch }
      saveSandbox(next)
      return next
    })
  }, [saveSandbox])

  const handleSelectFolder = async () => {
    const path = await window.loom.selectFolder()
    if (path) {
      setDefaultPath(path)
      await rpc('workspace.set_default', { path }, '默认工作空间已更新')
    }
  }

  const handleClear = async () => {
    setDefaultPath('')
    await rpc('workspace.set_default', { path: '' }, '默认工作空间已清除')
  }

  const handleResetSession = async (sid: string) => {
    useStore.getState().setSessionWorkspace(sid, defaultPath)
    await rpc('workspace.set_session', { session_id: sid, path: defaultPath }, '已重置为默认工作空间')
  }

  const sessionsWithWorkspace = Object.entries(sessionWorkspaces)
    .filter(([, path]) => path && path !== defaultPath)
    .map(([sid, path]) => {
      const session = sessions.find(s => s.path === sid)
      return { sid, path, title: session?.title || null }
    })

  if (loading) {
    return <p className={settingsStyles.toolsEmpty}>加载中...</p>
  }

  return (
    <div className={settingsStyles.aboutSection}>
      {/* ── 默认工作空间 ── */}
      <div className={settingsStyles.themeLabel}>默认工作空间</div>

      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>当前路径</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0, wordBreak: 'break-all', fontFamily: 'var(--font-mono)' }}>
            {defaultPath || '未设置 — 使用应用启动目录'}
          </p>
        </div>
      </div>

      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>工作目录</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>所有会话默认使用此目录，AI 在此创建和读取文件</p>
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          <button onClick={handleSelectFolder} className={settingsStyles.mcpTransportBtn} style={defaultPath ? {} : { background: 'var(--accent)', color: '#fff', borderColor: 'var(--accent)' }}>
            选择文件夹
          </button>
          {defaultPath && (
            <button onClick={handleClear} className={settingsStyles.mcpTransportBtn}>
              清除
            </button>
          )}
        </div>
      </div>

      <hr className={settingsStyles.sectionDivider} />

      {/* ── 会话工作空间 ── */}
      <div className={settingsStyles.themeLabel}>会话工作空间</div>

      {sessionsWithWorkspace.length === 0 ? (
        <p className={settingsStyles.toolsEmpty}>暂无覆盖了工作空间的会话 — 右键侧边栏会话可快速设置</p>
      ) : (
        sessionsWithWorkspace.map(({ sid, path, title }) => (
          <div key={sid} className={settingsStyles.aboutRow}>
            <div>
              <span className={settingsStyles.aboutLabel}>{title || sid.slice(0, 8)}</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 320 }}>
                {path}
              </p>
            </div>
            <button onClick={() => handleResetSession(sid)} className={settingsStyles.mcpTransportBtn}>
              重置为默认
            </button>
          </div>
        ))
      )}

      <hr className={settingsStyles.sectionDivider} />

      {/* ── 文件沙盒 ── */}
      <div className={settingsStyles.themeLabel}>文件沙盒</div>

      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>启用沙盒</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>限制 AI 对文件系统的访问范围，防止误删或泄露敏感文件</p>
        </div>
        <div className={settingsStyles.mcpTransportToggle}>
          <button
            className={`${settingsStyles.mcpTransportBtn} ${sandbox.enabled ? settingsStyles.mcpTransportActive : ''}`}
            onClick={() => updateSandbox({ enabled: true })}
          >
            开启
          </button>
          <button
            className={`${settingsStyles.mcpTransportBtn} ${!sandbox.enabled ? settingsStyles.mcpTransportActive : ''}`}
            onClick={() => updateSandbox({ enabled: false })}
          >
            关闭
          </button>
        </div>
      </div>

      {sandbox.enabled && (
        <>
          <div className={settingsStyles.aboutRow}>
            <div>
              <span className={settingsStyles.aboutLabel}>仅限工作区</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>所有文件操作限制在工作区目录内</p>
            </div>
            <div className={settingsStyles.mcpTransportToggle}>
              <button
                className={`${settingsStyles.mcpTransportBtn} ${sandbox.workspace_only ? settingsStyles.mcpTransportActive : ''}`}
                onClick={() => updateSandbox({ workspace_only: true })}
              >
                开启
              </button>
              <button
                className={`${settingsStyles.mcpTransportBtn} ${!sandbox.workspace_only ? settingsStyles.mcpTransportActive : ''}`}
                onClick={() => updateSandbox({ workspace_only: false })}
              >
                关闭
              </button>
            </div>
          </div>

          <div className={settingsStyles.aboutRow}>
            <div>
              <span className={settingsStyles.aboutLabel}>额外允许的路径</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>工作区之外允许访问的目录，每行一个</p>
            </div>
          </div>
          <textarea
            className={styles.pathsTextarea}
            rows={3}
            placeholder="/home/user/projects/other&#10;/tmp/build-output"
            value={sandbox.allowed_paths.join('\n')}
            onChange={(e) => updateSandbox({ allowed_paths: e.target.value.split('\n').filter(Boolean) })}
          />

          <div className={settingsStyles.aboutRow}>
            <div>
              <span className={settingsStyles.aboutLabel}>额外拒绝的路径</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>始终禁止访问的路径（优先级最高）</p>
            </div>
          </div>
          <textarea
            className={styles.pathsTextarea}
            rows={2}
            placeholder="/home/user/private&#10;/mnt/sensitive"
            value={sandbox.denied_paths.join('\n')}
            onChange={(e) => updateSandbox({ denied_paths: e.target.value.split('\n').filter(Boolean) })}
          />

          <div className={styles.builtinNote}>
            <span className={styles.builtinLabel}>内置禁止访问：</span>
            <span className={styles.builtinPatterns}>
              ~/.ssh, ~/.aws, .env, *.pem, *.key, *.p12, *.pfx, *.crt, *.jks, /etc/passwd, /etc/shadow, Windows\System32\config, .loom/
            </span>
          </div>
        </>
      )}

    </div>
  )
}
