import { useState, useEffect, useCallback } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useLocale } from '../../i18n'
import styles from './SettingsModal.module.css'

// Loom.md 全局纪律文件编辑器。
// 直接读写 ~/.loom/Loom.md，保存后对后续所有 agent 会话立即生效
// （build_full_system_prompt 每个 turn 重新读取，无需重启）。
export default function LoomMdSection() {
  const { t } = useLocale()
  const [content, setContent] = useState('')
  const [path, setPath] = useState('')
  const [loaded, setLoaded] = useState(false)
  const [saving, setSaving] = useState(false)
  const [dirty, setDirty] = useState(false)

  useEffect(() => {
    loomRpc<{ content: string; path: string }>('loom_md.read')
      .then((r) => {
        setContent(r.content ?? '')
        setPath(r.path ?? '')
        setLoaded(true)
      })
      .catch(() => setLoaded(true))
  }, [])

  const handleChange = (v: string) => {
    setContent(v)
    setDirty(true)
  }

  const save = useCallback(async () => {
    setSaving(true)
    try {
      await rpc('loom_md.save', { content }, t('settings.loomMdSaved'))
      setDirty(false)
    } catch {
      /* toast already shown by rpc helper */
    } finally {
      setSaving(false)
    }
  }, [content, t])

  if (!loaded) return null

  return (
    <div className={styles.globalDefaultsCard}>
      <h4 className={styles.globalDefaultsTitle}>{t('settings.loomMdTitle')}</h4>
      <p className={styles.globalDefaultsDesc}>{t('settings.loomMdHint')}</p>
      <textarea
        value={content}
        onChange={(e) => handleChange(e.target.value)}
        placeholder={t('settings.loomMdPlaceholder')}
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
