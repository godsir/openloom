// Write Settings Section — rendered inside the Settings page
// Manages font, AI completion, agent presets, quick actions, and workspace settings

import React, { useState } from 'react';
import { useWriteStore, WritePreviewMode } from '../../stores/write';
import { useLocale } from '../../i18n';

const FONT_OPTIONS = [
  { value: 'system', label: '系统默认' },
  { value: 'Microsoft YaHei', label: '微软雅黑' },
  { value: 'SimSun', label: '宋体' },
  { value: 'KaiTi', label: '楷体' },
  { value: 'SimHei', label: '黑体' },
];

const PREVIEW_MODES: { value: WritePreviewMode; label: string }[] = [
  { value: 'rich', label: '富文本' },
  { value: 'source', label: '源码' },
  { value: 'live', label: '实时' },
  { value: 'split', label: '分屏' },
  { value: 'preview', label: '预览' },
];

export const WriteSettingsSection: React.FC = () => {
  const { t } = useLocale();
  const store = useWriteStore();

  return (
    <div style={{ padding: '16px 20px', maxWidth: '600px' }}>
      <h2 style={{ fontSize: '16px', marginBottom: '20px' }}>{t('write.settings', '写作设置')}</h2>

      {/* Typography */}
      <Section title="排版设置">
        <Field label="默认字体">
          <select
            value={store.fontFamily}
            onChange={(e) => store.setFontFamily(e.target.value)}
            style={selectStyle}
          >
            {FONT_OPTIONS.map((f) => (
              <option key={f.value} value={f.value}>{f.label}</option>
            ))}
          </select>
        </Field>
        <Field label={`字号: ${store.fontSize}px`}>
          <input type="range" min={12} max={28} value={store.fontSize}
            onChange={(e) => store.setFontSize(Number(e.target.value))} style={{ width: '100%' }} />
        </Field>
        <Field label={`行高: ${store.lineHeight}`}>
          <input type="range" min={1.2} max={2.5} step={0.1} value={store.lineHeight}
            onChange={(e) => store.setLineHeight(Number(e.target.value))} style={{ width: '100%' }} />
        </Field>
        <Field label="默认预览模式">
          <div style={{ display: 'flex', gap: '2px' }}>
            {PREVIEW_MODES.map((m) => (
              <button key={m.value}
                onClick={() => store.setPreviewMode(m.value)}
                style={{
                  padding: '4px 10px', fontSize: '12px', border: '1px solid var(--border)',
                  background: store.previewMode === m.value ? 'var(--bg-active)' : 'transparent',
                  color: store.previewMode === m.value ? 'var(--text-accent)' : 'var(--text-muted)',
                  cursor: 'pointer', borderRadius: store.previewMode === m.value ? '4px' : '0',
                }}
              >{m.label}</button>
            ))}
          </div>
        </Field>
      </Section>

      {/* AI Completion */}
      <Section title="AI 补全设置">
        <Field label="内联补全">
          <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '13px' }}>
            <input type="checkbox" checked={store.inlineCompletionEnabled}
              onChange={(e) => store.setInlineCompletionEnabled(e.target.checked)} />
            启用 Ghost 文本补全
          </label>
        </Field>
        <Field label="补全延迟 (ms)">
          <input type="number" value={store.shortDebounceMs} min={150} max={5000} step={50}
            onChange={(e) => useWriteStore.setState({ shortDebounceMs: Number(e.target.value) })}
            style={inputStyle} />
        </Field>
        <Field label="长补全延迟 (ms)">
          <input type="number" value={store.longDebounceMs} min={1000} max={15000} step={100}
            onChange={(e) => useWriteStore.setState({ longDebounceMs: Number(e.target.value) })}
            style={inputStyle} />
        </Field>
      </Section>

      {/* Workspace */}
      <Section title="工作区">
        <Field label="RAG 检索">
          <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '13px' }}>
            <input type="checkbox" checked={store.retrievalEnabled}
              onChange={(e) => store.setRetrievalEnabled(e.target.checked)} />
            启用作区智能检索（Phase 3）
          </label>
        </Field>
        <Field label="自动保存间隔 (ms)">
          <input type="number" value={store.autoSaveIntervalMs} min={300} max={5000} step={100}
            onChange={(e) => useWriteStore.setState({ autoSaveIntervalMs: Number(e.target.value) })}
            style={inputStyle} />
        </Field>
      </Section>
    </div>
  );
};

// Helper sub-components
const Section: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
  <div style={{ marginBottom: '20px' }}>
    <h3 style={{ fontSize: '13px', fontWeight: 600, marginBottom: '10px', color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.5px' }}>{title}</h3>
    <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>{children}</div>
  </div>
);

const Field: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '12px' }}>
    <label style={{ fontSize: '13px', color: 'var(--text)', minWidth: '100px' }}>{label}</label>
    <div style={{ flex: 1 }}>{children}</div>
  </div>
);

const selectStyle: React.CSSProperties = {
  padding: '4px 8px', border: '1px solid var(--border)', borderRadius: '4px',
  background: 'var(--bg)', color: 'var(--text)', fontSize: '13px', width: '100%',
};

const inputStyle: React.CSSProperties = {
  padding: '4px 8px', border: '1px solid var(--border)', borderRadius: '4px',
  background: 'var(--bg)', color: 'var(--text)', fontSize: '13px', width: '100%',
};
