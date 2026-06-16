import { useWriteStore } from '../../stores/write';
import { useLocale } from '../../i18n';
import styles from './WriteSettingsSection.module.css';
import type { WritePreviewMode } from '../../stores/write';

const FONT_OPTIONS = [
  { value: 'system', label: '系统默认 / System Default' },
  { value: 'Microsoft YaHei', label: '微软雅黑' },
  { value: 'SimSun', label: '宋体' },
  { value: 'KaiTi', label: '楷体' },
  { value: 'SimHei', label: '黑体' },
  { value: 'Source Han Sans', label: '思源黑体' },
  { value: 'PingFang SC', label: '苹方' },
  { value: 'JetBrains Mono', label: 'JetBrains Mono' },
  { value: 'Fira Code', label: 'Fira Code' },
];

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

  return (
    <div className={styles.root}>
      {/* ---- Typography ---- */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>{t('write.settingsTypography', '排版设置')}</h3>
        <div className={styles.sectionCard}>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsDefaultFont', '默认字体')}</label>
            <select className={styles.select} value={store.fontFamily} onChange={(e) => store.setFontFamily(e.target.value)}>
              {FONT_OPTIONS.map((f) => (<option key={f.value} value={f.value}>{f.label}</option>))}
            </select>
          </div>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsFontSize', '字号') + ': ' + store.fontSize + 'px'}</label>
            <input type="range" style={{ flex: 1 }} min={12} max={28} value={store.fontSize}
              onChange={(e) => store.setFontSize(Number(e.target.value))} />
          </div>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsLineHeight', '行高') + ': ' + store.lineHeight.toFixed(1)}</label>
            <input type="range" style={{ flex: 1 }} min={1.2} max={2.5} step={0.1} value={store.lineHeight}
              onChange={(e) => store.setLineHeight(Number(e.target.value))} />
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
        </div>
      </div>

      {/* ---- AI Completion ---- */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>{t('write.settingsAICompletion', 'AI 补全设置')}</h3>
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
            <label className={styles.label}>{t('write.settingsCompletionDebounceShort', '补全延迟')}</label>
            <input type="number" className={styles.numInput}
              value={store.shortDebounceMs} min={150} max={5000} step={50}
              onChange={(e) => useWriteStore.setState({ shortDebounceMs: Number(e.target.value) || 300 })} />
          </div>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsCompletionDebounceLong', '长补全延迟')}</label>
            <input type="number" className={styles.numInput}
              value={store.longDebounceMs} min={1000} max={15000} step={100}
              onChange={(e) => useWriteStore.setState({ longDebounceMs: Number(e.target.value) || 1500 })} />
          </div>
        </div>
      </div>

      {/* ---- Workspace ---- */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>{t('write.settingsWorkspace', '工作区')}</h3>
        <div className={styles.sectionCard}>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsRetrieval', '工作区检索')}</label>
            <label className={styles.checkboxLabel}>
              <input type="checkbox" checked={store.retrievalEnabled}
                onChange={(e) => store.setRetrievalEnabled(e.target.checked)} />
              <span>{t('write.settingsRetrievalDesc', '启用 BM25 关键词检索增强')}</span>
            </label>
          </div>
          <div className={styles.fieldRow}>
            <label className={styles.label}>{t('write.settingsAutoSaveInterval', '自动保存间隔')}</label>
            <input type="number" className={styles.numInput}
              value={store.autoSaveIntervalMs} min={300} max={5000} step={100}
              onChange={(e) => useWriteStore.setState({ autoSaveIntervalMs: Number(e.target.value) || 900 })} />
          </div>
        </div>
      </div>
    </div>
  );
};
