import React from 'react'
import { useWriteStore } from '../../stores/write'
import { useLocale } from '../../i18n'
import { IconFolder, IconFile } from '../../utils/icons'

interface WriteWorkspaceStartProps {
  onSelectWorkspace: () => void;
}

/**
 * Landing page shown when no file is open.
 * Two states: no workspace (prompts to pick one) / workspace selected but no file open.
 */
export const WriteWorkspaceStart: React.FC<WriteWorkspaceStartProps> = ({ onSelectWorkspace }) => {
  const workspaceRoot = useWriteStore(s => s.workspaceRoot)
  const { t } = useLocale()

  if (!workspaceRoot) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', gap: '12px', color: 'var(--text-muted)' }}>
        <IconFolder size={48} style={{ opacity: 0.15 }} />
        <span style={{ fontSize: '14px' }}>{t('write.selectDirStart')}</span>
        <button
          onClick={(e) => { e.stopPropagation(); onSelectWorkspace() }}
          style={{ padding: '8px 20px', border: '1px solid var(--border-accent)', borderRadius: '8px', background: 'var(--accent-subtle)', color: 'var(--accent)', cursor: 'pointer', fontSize: '13px', fontWeight: 500 }}
        >
          {t('write.selectDirectory')}
        </button>
      </div>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', gap: '8px', color: 'var(--text-muted)' }}>
      <IconFile size={40} style={{ opacity: 0.15 }} />
      <span style={{ fontSize: '14px' }}>{t('write.selectOrNewFilePrompt')}</span>
    </div>
  )
}
