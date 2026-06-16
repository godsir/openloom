import { useWriteStore } from '../../stores/write';
import { useLocale } from '../../i18n';
import styles from './WriteSettingsSection.module.css';
import type { WritePreviewMode } from '../../stores/write';

const PREVIEW_MODES: { value: WritePreviewMode; labelKey: string; label: string }[] = [
  { value: 'rich', labelKey: 'write.previewRich', label: '所见即所得' },
  { value: 'source', labelKey: 'write.previewEdit', label: '源码' },
  { value: 'live', labelKey: 'write.previewLive', label: '实时' },
  { value: 'split', labelKey: 'write.previewSplit', label: '分屏' },
  { value: 'preview', labelKey: 'write.previewPreview', label: '预览' },
];

export const WriteSettingsSection: React.FC = () => {
  const { t } = useLocale();
  const store = useWriteStore();

  const handlePickWorkspace = async () => {
    try {
      const p = await (window as any).loom?.selectFolder?.();
      if (p) store.setWorkspaceRoot(p);
    } catch {}
  };

  return (
    <div className={styles.root}>
      {/* ---- 工作区 ---- */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>{t('write.settingsWorkspace', '工作区')}</h3>
        <div className={styles.sectionCard}>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsDefaultWorkspace', '默认工作区')}</label>
            <div style={{ flex: 1, display: 'flex', alignItems: 'center', gap: 8 }}>
              <span style={{ flex: 1, fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {store.workspaceRoot || t('write.noWorkspace', '未设置')}
              </span>
              <button className={styles.actionBtn} onClick={handlePickWorkspace}>
                {t('write.selectDirectory', '选择...')}
              </button>
            </div>
          </div>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsDefaultPreview', '默认预览模式')}</label>
            <div className={styles.modeToggle}>
              {PREVIEW_MODES.map((m) => (
                <button key={m.value}
                  className={store.previewMode === m.value ? styles.toggleBtnActive : styles.toggleBtn}
                  onClick={() => store.setPreviewMode(m.value)}>
                  {t(m.labelKey, m.label)}
                </button>
              ))}
            </div>
          </div>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsAutoSaveInterval', '自动保存间隔')}</label>
            <input type="number" className={styles.numInput}
              value={store.autoSaveIntervalMs} min={300} max={5000} step={100}
              onChange={(e) => useWriteStore.setState({ autoSaveIntervalMs: Number(e.target.value) || 900 })} />
          </div>
        </div>
      </div>

      {/* ---- AI 补全 ---- */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>{t('write.settingsAICompletion', 'AI 补全')}</h3>
        <div className={styles.sectionCard}>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsInlineCompletion', '内联补全')}</label>
            <label className={styles.checkboxLabel}>
              <input type="checkbox" checked={store.inlineCompletionEnabled}
                onChange={(e) => store.setInlineCompletionEnabled(e.target.checked)} />
              <span>{t('write.settingsInlineCompletionDesc', '启用 Ghost 文本补全')}</span>
            </label>
          </div>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsRetrieval', '工作区检索')}</label>
            <label className={styles.checkboxLabel}>
              <input type="checkbox" checked={store.retrievalEnabled}
                onChange={(e) => store.setRetrievalEnabled(e.target.checked)} />
              <span>{t('write.settingsRetrievalDesc', '启用 BM25 关键词检索增强')}</span>
            </label>
          </div>
        </div>
      </div>
    </div>
  );
};
