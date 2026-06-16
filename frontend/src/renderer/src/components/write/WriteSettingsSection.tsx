import { useWriteStore, WritePreviewMode } from '../../stores/write';
import { useLocale } from '../../i18n';
import styles from './WriteSettingsSection.module.css';

// ---- constants ----

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

// ---- helper sub-components ----

const Section: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
  <div className={styles.section}>
    <h3 className={styles.sectionTitle}>{title}</h3>
    <div className={styles.fieldGroup}>{children}</div>
  </div>
);

const Field: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div className={styles.field}>
    <label className={styles.fieldLabel}>{label}</label>
    <div className={styles.fieldValue}>{children}</div>
  </div>
);

// ---- main component ----

export const WriteSettingsSection: React.FC = () => {
  const { t } = useLocale();
  const store = useWriteStore();

  return (
    <div>
      <h2 className={styles.pageTitle}>{t('write.settings', '写作设置')}</h2>

      {/* ---- Typography ---- */}
      <Section title={t('write.settingsTypography', '排版设置')}>
        <Field label={t('write.settingsDefaultFont', '默认字体')}>
          <select
            className={styles.select}
            value={store.fontFamily}
            onChange={(e) => store.setFontFamily(e.target.value)}
          >
            {FONT_OPTIONS.map((f) => (
              <option key={f.value} value={f.value}>{f.label}</option>
            ))}
          </select>
        </Field>

        <Field label={t('write.settingsFontSize', '字号') + `: ${store.fontSize}px`}>
          <input type="range" className={styles.range} min={12} max={28} value={store.fontSize}
            onChange={(e) => store.setFontSize(Number(e.target.value))} />
        </Field>

        <Field label={t('write.settingsLineHeight', '行高') + `: ${store.lineHeight.toFixed(1)}`}>
          <input type="range" className={styles.range} min={1.2} max={2.5} step={0.1} value={store.lineHeight}
            onChange={(e) => store.setLineHeight(Number(e.target.value))} />
        </Field>

        <Field label={t('write.settingsDefaultPreview', '默认预览模式')}>
          <div className={styles.modeRow}>
            {PREVIEW_MODES.map((m) => (
              <button
                key={m.value}
                className={store.previewMode === m.value ? styles.modeBtnActive : styles.modeBtn}
                onClick={() => store.setPreviewMode(m.value)}
              >
                {t(m.labelKey, m.label)}
              </button>
            ))}
          </div>
        </Field>
      </Section>

      {/* ---- AI Completion ---- */}
      <Section title={t('write.settingsAICompletion', 'AI 补全设置')}>
        <Field label={t('write.settingsInlineCompletion', '内联补全')}>
          <label className={styles.checkboxLabel}>
            <input type="checkbox" checked={store.inlineCompletionEnabled}
              onChange={(e) => store.setInlineCompletionEnabled(e.target.checked)} />
            {t('write.settingsInlineCompletionDesc', '启用 Ghost 文本补全')}
          </label>
        </Field>

        <Field label={t('write.settingsCompletionDebounceShort', '补全延迟')}>
          <input type="number" className={styles.input}
            value={store.shortDebounceMs} min={150} max={5000} step={50}
            onChange={(e) => useWriteStore.setState({ shortDebounceMs: Number(e.target.value) || 300 })} />
        </Field>

        <Field label={t('write.settingsCompletionDebounceLong', '长补全延迟')}>
          <input type="number" className={styles.input}
            value={store.longDebounceMs} min={1000} max={15000} step={100}
            onChange={(e) => useWriteStore.setState({ longDebounceMs: Number(e.target.value) || 1500 })} />
        </Field>
      </Section>

      {/* ---- Workspace ---- */}
      <Section title={t('write.settingsWorkspace', '工作区')}>
        <Field label={t('write.settingsRetrieval', '工作区检索')}>
          <label className={styles.checkboxLabel}>
            <input type="checkbox" checked={store.retrievalEnabled}
              onChange={(e) => store.setRetrievalEnabled(e.target.checked)} />
            {t('write.settingsRetrievalDesc', '启用 BM25 关键词检索增强')}
          </label>
        </Field>

        <Field label={t('write.settingsAutoSaveInterval', '自动保存间隔')}>
          <input type="number" className={styles.input}
            value={store.autoSaveIntervalMs} min={300} max={5000} step={100}
            onChange={(e) => useWriteStore.setState({ autoSaveIntervalMs: Number(e.target.value) || 900 })} />
        </Field>
      </Section>
    </div>
  );
};
