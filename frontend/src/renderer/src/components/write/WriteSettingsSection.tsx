import { useWriteStore } from '../../stores/write';
import { useStore } from '../../stores';
import { useLocale } from '../../i18n';
import styles from './WriteSettingsSection.module.css';
import sharedStyles from '../shared/SettingsModal.module.css';
import Select from '../shared/Select';
import type { WritePreviewMode } from '../../stores/write';
import { guardWriteNavigation } from '../../write/navigation-guard';

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
  const agents = useStore((s) => s.agents);
  const writingAgents = agents.filter((agent) =>
    agent.name && agent.name !== 'default' && !agent.name.startsWith('__team_')
  );

  const handlePickWorkspace = async () => {
    try {
      const p = await (window as any).loom?.selectFolder?.();
      console.log('[WriteSettings] selectFolder returned:', p);
      if (p && p !== store.workspaceRoot && await guardWriteNavigation()) store.setWorkspaceRoot(p);
    } catch (e) { console.log('[WriteSettings] handlePickWorkspace error:', e); }
  };

  return (
    <div className={styles.root}>
      {/* ── 工作区设置 ── */}
      <div className={sharedStyles.aboutSection}>
        <div className={sharedStyles.themeLabel}>{t('write.settingsWorkspace', '工作区')}</div>

        <div className={styles.fieldRow}>
          <div className={styles.fieldInfo}>
            <p className={styles.fieldLabel}>{t('write.settingsDefaultWorkspace', '默认工作区')}</p>
            <p className={styles.fieldDesc}>
              {store.workspaceRoot || t('write.noWorkspace', '未设置')}
            </p>
          </div>
          <div className={styles.fieldAction}>
            <button className={sharedStyles.mcpAddBtn} onClick={handlePickWorkspace}>
              {t('write.selectDirectory', '选择...')}
            </button>
          </div>
        </div>

        <div className={styles.fieldRow}>
          <div className={styles.fieldInfo}>
            <p className={styles.fieldLabel}>{t('write.settingsDefaultPreview', '默认预览模式')}</p>
            <p className={styles.fieldDesc}>{t('write.settingsDefaultPreviewDesc', '新建文档时的默认编辑模式')}</p>
          </div>
          <div className={styles.fieldAction}>
            <div className={sharedStyles.mcpTransportToggle}>
              {PREVIEW_MODES.map((m) => (
                <button key={m.value}
                  className={`${sharedStyles.mcpTransportBtn} ${store.previewMode === m.value ? sharedStyles.mcpTransportActive : ''}`}
                  onClick={() => store.setPreviewMode(m.value)}>
                  {t(m.labelKey, m.label)}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className={styles.fieldRow}>
          <div className={styles.fieldInfo}>
            <p className={styles.fieldLabel}>{t('write.settingsAutoSaveInterval', '自动保存间隔')}</p>
            <p className={styles.fieldDesc}>{t('write.settingsAutoSaveIntervalDesc', '文档自动保存的时间间隔')}</p>
          </div>
          <div className={styles.fieldAction}>
            <input type="number" className={styles.numInput}
              value={store.autoSaveIntervalMs} min={300} max={5000} step={100}
              onChange={(e) => useWriteStore.setState({ autoSaveIntervalMs: Number(e.target.value) || 900 })} />
            <span className={styles.unitLabel}>ms</span>
          </div>
        </div>
      </div>

      <hr className={sharedStyles.sectionDivider} />

      {/* ── AI 补全设置 ── */}
      <div className={sharedStyles.aboutSection}>
        <div className={sharedStyles.themeLabel}>{t('write.settingsAICompletion', 'AI 补全')}</div>

        <div className={styles.fieldRow}>
          <div className={styles.fieldInfo}>
            <p className={styles.fieldLabel}>{t('write.settingsAgent', '写作专属 Agent')}</p>
            <p className={styles.fieldDesc}>{t('write.settingsAgentDesc', '写作会话将自动使用该 Agent；精简模式下仍保留其核心写作要求')}</p>
          </div>
          <div className={styles.fieldAction}>
            <Select
              value={store.writingAgentName || ''}
              options={[
                { value: '', label: t('write.settingsAgentDefault', '默认 Agent') },
                ...writingAgents.map((agent) => ({ value: agent.name, label: agent.name })),
              ]}
              onChange={(value) => store.setWritingAgentName(value || null)}
              variant="form"
              menuWidth={220}
            />
          </div>
        </div>

        <div className={styles.fieldRow}>
          <div className={styles.fieldInfo}>
            <p className={styles.fieldLabel}>{t('write.settingsRetrieval', '工作区检索')}</p>
            <p className={styles.fieldDesc}>{t('write.settingsRetrievalDesc', '启用按工作区隔离的 BM25 检索增强')}</p>
          </div>
          <div className={styles.fieldAction}>
            <label className={styles.checkboxLabel}>
              <input type="checkbox" checked={store.retrievalEnabled}
                onChange={(e) => store.setRetrievalEnabled(e.target.checked)} />
            </label>
          </div>
        </div>

        <div className={styles.fieldRow}>
          <div className={styles.fieldInfo}>
            <p className={styles.fieldLabel}>{t('write.settingsInlineCompletion', '内联补全')}</p>
            <p className={styles.fieldDesc}>{t('write.settingsInlineCompletionDesc', '启用 Ghost 文本补全')}</p>
          </div>
          <div className={styles.fieldAction}>
            <label className={styles.checkboxLabel}>
              <input type="checkbox" checked={store.inlineCompletionEnabled}
                onChange={(e) => store.setInlineCompletionEnabled(e.target.checked)} />
            </label>
          </div>
        </div>

      </div>
    </div>
  );
};
