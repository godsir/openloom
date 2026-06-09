import { useStore } from '../../stores'
import Overlay from './Overlay'
import { useLocale } from '../../i18n'

export default function ArchivedSessionsModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useLocale()
  const sessions = useStore((s) => s.sessions)

  return (
    <Overlay open={open} onClose={onClose} title={t('sessions.archived')}>
      {sessions.length === 0 ? (
        <p className="text-sm text-[var(--text-muted)] text-center py-8">{t('sessions.noArchived')}</p>
      ) : (
        <div className="space-y-1 max-h-64 overflow-y-auto">
          {sessions.map((s) => (
            <div
              key={s.path}
              className="flex items-center gap-2 px-3.5 py-2.5 bg-[var(--bg-card)] rounded-[var(--r-sm)] text-sm border border-[var(--border)]"
            >
              <span className="flex-1 truncate text-[var(--text-light)]">
                {s.title || s.path.slice(0, 8)}
              </span>
              <span className="text-[11px] font-mono text-[var(--text-muted)]">
                {t('sidebar.messageCount', { n: s.messageCount ?? 0 })}
              </span>
              <span className="text-[10px] font-mono text-[var(--text-muted)]">
                {new Date(s.modified).toLocaleDateString(navigator.language)}
              </span>
            </div>
          ))}
        </div>
      )}
    </Overlay>
  )
}
