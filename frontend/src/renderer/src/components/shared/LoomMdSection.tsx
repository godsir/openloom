import { useState, useEffect, useCallback } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useLocale } from '../../i18n'
import styles from './SettingsModal.module.css'

// Loom.md 编辑器 — 统一通过 loom_md.read/save 读写。
// - workspaceRoot 为空 → 全局 ~/.loom/Loom.md
// - workspaceRoot 有值 → $workspace_root/Loom.md（不存在时后端自动创建空文件）

export default function LoomMdSection({ workspaceRoot }: { workspaceRoot?: string }) {
  const { t } = useLocale()
  const [content, setContent] = useState('')
  const [path, setPath] = useState('')
  const [loaded, setLoaded] = useState(false)
  const [saving, setSaving] = useState(false)
  const [dirty, setDirty] = useState(false)

  const isWorkspace = !!workspaceRoot

  useEffect(() => {
    let cancelled = false
    const params: Record<string, unknown> = {}
    if (isWorkspace) {
      params.workspace_root = workspaceRoot
    }
    loomRpc<{ content: string; path: string }>('loom_md.read', params)
      .then((r) => {
        if (cancelled) return
        setContent(r.content ?? '')
        setPath(r.path ?? '')
        setLoaded(true)
      })
      .catch(() => { if (!cancelled) setLoaded(true) })
    return () => { cancelled = true }
  }, [workspaceRoot, isWorkspace])

  const handleChange = (v: string) => {
    setContent(v)
    setDirty(true)
  }

  const save = useCallback(async () => {
    setSaving(true)
    try {
      const params: Record<string, unknown> = { content }
      if (isWorkspace) {
        params.workspace_root = workspaceRoot
      }
      await rpc('loom_md.save', params, t('settings.loomMdSaved'))
      setDirty(false)
    } catch {
      /* toast already shown by rpc helper */
    } finally {
      setSaving(false)
    }
  }, [content, t, isWorkspace, workspaceRoot])

  if (!loaded) return null

  const title = isWorkspace ? t('settings.loomMdWorkspaceTitle') : t('settings.loomMdTitle')
  const hint = isWorkspace ? t('settings.loomMdWorkspaceHint') : t('settings.loomMdHint')
  const placeholder = t('settings.loomMdPlaceholder')

  return (
    <div className={styles.globalDefaultsCard}>
      <h4 className={styles.globalDefaultsTitle}>{title}</h4>
      <p className={styles.globalDefaultsDesc}>{hint}</p>
      <textarea
        value={content}
        onChange={(e) => handleChange(e.target.value)}
        placeholder={placeholder}
        spellCheck={false}
        className={styles.loomMdTextarea}
      />
      <div className={styles.loomMdFooter}>
        <span className={styles.globalDefaultsHint} title={path}>{path}</span>
        <button
          onClick={save}
          disabled={saving || !dirty}
          className={styles.globalDefaultsSaveBtn}
        >
          {saving ? t('settings.saving') : t('common.save')}
        </button>
      </div>
    </div>
  )
}
