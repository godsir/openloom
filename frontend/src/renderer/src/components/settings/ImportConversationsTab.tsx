import { useState, useEffect, useCallback } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { rpc } from '../../services/rpc-toast'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { IconSearch, IconDownload, IconRefresh, IconLoader } from '../../utils/icons'
import { toggleId, allSelected } from './importSelection'
import styles from './ImportConversationsTab.module.css'

interface ConvSummary {
  session_uuid: string
  project_dir: string
  title: string | null
  first_message: string | null
  message_count: number
  model: string | null
  started_at: string
  last_at: string
  already_imported: boolean
}

export default function ImportConversationsTab() {
  const { t } = useLocale()
  const loadSessions = useStore((s) => s.loadSessions)
  const [convs, setConvs] = useState<ConvSummary[]>([])
  const [selected, setSelected] = useState<string[]>([])
  const [scanning, setScanning] = useState(false)
  const [importing, setImporting] = useState(false)
  const [query, setQuery] = useState('')

  const scan = useCallback(async () => {
    setScanning(true)
    try {
      const r = await loomRpc<{ conversations: ConvSummary[] }>('claude_import.scan')
      setConvs(r.conversations ?? [])
      setSelected([])
    } finally {
      setScanning(false)
    }
  }, [])

  useEffect(() => { scan() }, [scan])

  const filtered = convs.filter((c) => {
    if (!query) return true
    const q = query.toLowerCase()
    return (c.title ?? '').toLowerCase().includes(q)
      || (c.first_message ?? '').toLowerCase().includes(q)
      || c.project_dir.toLowerCase().includes(q)
  })

  const groups = filtered.reduce<Record<string, ConvSummary[]>>((acc, c) => {
    (acc[c.project_dir] ??= []).push(c)
    return acc
  }, {})

  const importable = filtered.filter((c) => !c.already_imported)
  const allSel = allSelected(importable.map((c) => c.session_uuid), selected)

  const importSelected = async () => {
    setImporting(true)
    try {
      await rpc('claude_import.run', { ids: selected }, t('settings.importDone', '导入完成'))
      await scan()
      await loadSessions()
    } catch {
      // rpc() already toasted
    } finally {
      setImporting(false)
    }
  }

  return (
    <div className={styles.wrap}>
      <div className={styles.header}>
        <h3 className={styles.title}>{t('settings.importConversations', '导入 Claude Code 对话')}</h3>
      </div>
      <div className={styles.toolbar}>
        <IconSearch size={14} />
        <input
          className={styles.search}
          placeholder={t('settings.searchConversations', '搜索标题 / 项目')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button className={styles.scanBtn} onClick={scan} disabled={scanning}>
          {scanning ? <IconLoader size={14} /> : <IconRefresh size={14} />}
          {t('settings.rescan', '重新扫描')}
        </button>
      </div>

      {filtered.length === 0 && (
        <div className={styles.empty}>
          {scanning
            ? (t('settings.scanning', '扫描中…'))
            : (t('settings.noConversations', '未发现 Claude Code 对话（路径：~/.claude/projects）'))}
        </div>
      )}

      {Object.entries(groups).map(([dir, items]) => (
        <div key={dir} className={styles.group}>
          <div className={styles.groupLabel}>{dir}</div>
          {items.map((c) => {
            const disabled = c.already_imported
            const checked = selected.includes(c.session_uuid)
            return (
              <label
                key={c.session_uuid}
                className={`${styles.row} ${disabled ? styles.rowDisabled : ''}`}
              >
                <input
                  type="checkbox"
                  disabled={disabled}
                  checked={checked}
                  onChange={() => setSelected((s) => toggleId(s, c.session_uuid))}
                />
                <div className={styles.rowContent}>
                  <span>{c.title || c.first_message || c.session_uuid}</span>
                  <span className={styles.meta}>
                    {c.message_count} 条 · {c.model ?? '?'} · {c.started_at.slice(0, 10)}
                    {disabled ? ` · ${t('settings.imported', '已导入')}` : ''}
                  </span>
                </div>
              </label>
            )
          })}
        </div>
      ))}

      <button
        className={styles.importBtn}
        onClick={importSelected}
        disabled={selected.length === 0 || importing}
      >
        {importing ? <IconLoader size={14} /> : <IconDownload size={14} />}
        {t('settings.importSelected', '导入选中')} ({selected.length})
      </button>
    </div>
  )
}
