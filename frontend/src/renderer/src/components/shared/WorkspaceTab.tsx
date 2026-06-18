import { useState, useEffect, useRef, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useLocale } from '../../i18n'
import LoomMdSection from './LoomMdSection'
import { IconFileText } from '../../utils/icons'
import settingsStyles from './SettingsModal.module.css'
import styles from './WorkspaceTab.module.css'

interface SandboxConfig {
  enabled: boolean
  workspace_only: boolean
  allowed_paths: string[]
  denied_paths: string[]
  allow_loom_data: boolean
}

export default function WorkspaceTab() {
  const { t } = useLocale()
  const [defaultPath, setDefaultPath] = useState('')
  const [loading, setLoading] = useState(true)
  const [sandbox, setSandbox] = useState<SandboxConfig>({ enabled: false, workspace_only: true, allowed_paths: [], denied_paths: [], allow_loom_data: false })
  const sandboxRef = useRef(sandbox)
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const sessions = useStore(s => s.sessions)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const [editingLoomMd, setEditingLoomMd] = useState<string | null>(null) // workspace path being edited, or null

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
      await rpc('workspace.set_default', { path }, t('workspace.workspaceSet'))
    }
  }

  const handleClear = async () => {
    setDefaultPath('')
    await rpc('workspace.set_default', { path: '' }, t('workspace.workspaceCleared'))
  }

  const handleResetSession = async (sid: string) => {
    useStore.getState().setSessionWorkspace(sid, defaultPath)
    await rpc('workspace.set_session', { session_id: sid, path: defaultPath }, t('workspace.resetToDefaultDone'))
  }

  const sessionsWithWorkspace = Object.entries(sessionWorkspaces)
    .filter(([, path]) => path && path !== defaultPath)
    .map(([sid, path]) => {
      const session = sessions.find(s => s.path === sid)
      return { sid, path, title: session?.title || null, key: path as string }
    })

  if (loading) {
    return <p className={settingsStyles.toolsEmpty}>{t('common.loading')}</p>
  }

  return (
    <div className={settingsStyles.aboutSection}>
      {/* ── Default workspace ── */}
      <div className={settingsStyles.themeLabel}>{t('workspace.defaultWorkspace')}</div>

      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>{t('workspace.currentPath')}</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0, wordBreak: 'break-all', fontFamily: 'var(--font-mono)' }}>
            {defaultPath || t('workspace.notSet')}
          </p>
        </div>
      </div>

      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>{t('workspace.workDir')}</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('workspace.workDirDesc')}</p>
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          <button onClick={handleSelectFolder} className={settingsStyles.mcpTransportBtn} style={defaultPath ? {} : { background: 'var(--accent)', color: '#fff', borderColor: 'var(--accent)' }}>
            {t('workspace.selectFolder')}
          </button>
          {defaultPath && (
            <>
              <button
                onClick={() => setEditingLoomMd(editingLoomMd === defaultPath ? null : defaultPath)}
                className={settingsStyles.mcpTransportBtn}
                title={t('settings.loomMd')}
                style={editingLoomMd === defaultPath ? { background: 'var(--accent)', color: '#fff', borderColor: 'var(--accent)' } : {}}
              >
                <IconFileText size={14} />
              </button>
              <button onClick={handleClear} className={settingsStyles.mcpTransportBtn}>
                {t('common.clear')}
              </button>
            </>
          )}
        </div>
      </div>

      {/* Default workspace Loom.md expand */}
      {editingLoomMd === defaultPath && defaultPath && (
        <div style={{ marginTop: 12 }}>
          <LoomMdSection workspaceRoot={defaultPath} />
        </div>
      )}

      <hr className={settingsStyles.sectionDivider} />

      {/* ── Session workspaces ── */}
      <div className={settingsStyles.themeLabel}>{t('workspace.sessionWorkspace')}</div>

      {sessionsWithWorkspace.length === 0 ? (
        <p className={settingsStyles.toolsEmpty}>{t('workspace.noOverridesDesc')}</p>
      ) : (
        sessionsWithWorkspace.map(({ sid, path, title, key }) => (
          <div key={sid}>
            <div className={settingsStyles.aboutRow}>
              <div>
                <span className={settingsStyles.aboutLabel}>{title || sid.slice(0, 8)}</span>
                <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 320 }}>
                  {path}
                </p>
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                <button
                  onClick={() => setEditingLoomMd(editingLoomMd === key ? null : key)}
                  className={settingsStyles.mcpTransportBtn}
                  title={t('settings.loomMd')}
                  style={editingLoomMd === key ? { background: 'var(--accent)', color: '#fff', borderColor: 'var(--accent)' } : {}}
                >
                  <IconFileText size={14} />
                </button>
                <button onClick={() => handleResetSession(sid)} className={settingsStyles.mcpTransportBtn}>
                  {t('workspace.resetToDefault')}
                </button>
              </div>
            </div>
            {editingLoomMd === key && (
              <div style={{ marginTop: 12 }}>
                <LoomMdSection workspaceRoot={path} />
              </div>
            )}
          </div>
        ))
      )}

      <hr className={settingsStyles.sectionDivider} />

      {/* File sandbox */}
      <div className={settingsStyles.themeLabel}>{t('workspace.sandbox')}</div>

      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>{t('workspace.sandboxEnable') || t('workspace.sandbox')}</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('workspace.sandboxDesc')}</p>
        </div>
        <div className={settingsStyles.mcpTransportToggle}>
          <button
            className={`${settingsStyles.mcpTransportBtn} ${sandbox.enabled ? settingsStyles.mcpTransportActive : ''}`}
            onClick={() => updateSandbox({ enabled: true })}
          >
            {t('workspace.sandboxOn')}
          </button>
          <button
            className={`${settingsStyles.mcpTransportBtn} ${!sandbox.enabled ? settingsStyles.mcpTransportActive : ''}`}
            onClick={() => updateSandbox({ enabled: false })}
          >
            {t('workspace.sandboxOff')}
          </button>
        </div>
      </div>

      {sandbox.enabled && (
        <>
          <div className={settingsStyles.aboutRow}>
            <div>
              <span className={settingsStyles.aboutLabel}>{t('workspace.wsOnly')}</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('workspace.wsOnlyDesc')}</p>
            </div>
            <div className={settingsStyles.mcpTransportToggle}>
              <button
                className={`${settingsStyles.mcpTransportBtn} ${sandbox.workspace_only ? settingsStyles.mcpTransportActive : ''}`}
                onClick={() => updateSandbox({ workspace_only: true })}
              >
                {t('workspace.sandboxOn')}
              </button>
              <button
                className={`${settingsStyles.mcpTransportBtn} ${!sandbox.workspace_only ? settingsStyles.mcpTransportActive : ''}`}
                onClick={() => updateSandbox({ workspace_only: false })}
              >
                {t('workspace.sandboxOff')}
              </button>
            </div>
          </div>

          <div className={settingsStyles.aboutRow}>
            <div>
              <span className={settingsStyles.aboutLabel}>{t('workspace.allowLoomData')}</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('workspace.allowLoomDataDesc')}</p>
            </div>
            <div className={settingsStyles.mcpTransportToggle}>
              <button
                className={`${settingsStyles.mcpTransportBtn} ${sandbox.allow_loom_data ? settingsStyles.mcpTransportActive : ''}`}
                onClick={() => updateSandbox({ allow_loom_data: true })}
              >
                {t('workspace.sandboxOn')}
              </button>
              <button
                className={`${settingsStyles.mcpTransportBtn} ${!sandbox.allow_loom_data ? settingsStyles.mcpTransportActive : ''}`}
                onClick={() => updateSandbox({ allow_loom_data: false })}
              >
                {t('workspace.sandboxOff')}
              </button>
            </div>
          </div>

          <div className={settingsStyles.aboutRow}>
            <div>
              <span className={settingsStyles.aboutLabel}>{t('workspace.extraAllowed')}</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('workspace.extraAllowedDesc')}</p>
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
              <span className={settingsStyles.aboutLabel}>{t('workspace.extraDenied')}</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('workspace.extraDeniedDesc')}</p>
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
            <span className={styles.builtinLabel}>{t('workspace.builtinDeny')}</span>
            <span className={styles.builtinPatterns}>
              ~/.ssh, ~/.aws, .env, *.pem, *.key, *.p12, *.pfx, *.crt, *.jks, /etc/passwd, /etc/shadow, Windows\System32\config{sandbox.allow_loom_data ? '' : ', .loom/'}
            </span>
          </div>
        </>
      )}
    </div>
  )
}
