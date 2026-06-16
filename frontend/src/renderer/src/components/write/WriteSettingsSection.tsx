// Write Settings Section — rendered inside the Settings page
// Manages font, AI completion, agent presets, quick actions, and workspace settings

import React from 'react';
import { useWriteStore, WritePreviewMode } from '../../stores/write';
import { useTranslation } from 'react-i18next';

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

const sectionTitleStyle: React.CSSProperties = {
  fontSize: '13px', fontWeight: 600, marginBottom: '10px',
  color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.5px',
};

const fieldStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '12px', marginBottom: '8px',
};

const labelStyle: React.CSSProperties = {
  fontSize: '13px', color: 'var(--text)', minWidth: '100px',
};

const inputStyle: React.CSSProperties = {
  padding: '4px 8px', border: '1px solid var(--border)', borderRadius: '4px',
  background: 'var(--bg)', color: 'var(--text)', fontSize: '13px', flex: 1,
};

const selectStyle: React.CSSProperties = {
  ...inputStyle, width: '100%',
};

export const WriteSettingsSection: React.FC = () => {
  const { t } = useTranslation();
  const store = useWriteStore();

  return (
    <div style={{ padding: '16px 20px', maxWidth: '600px' }}>
      <h2 style={{ fontSize: '16px', marginBottom: '20px' }}>
        {t('write.settings', '写作设置')}
      </h2>

      {/* Typography */}
      <div style={{ marginBottom: '20px' }}>
        <h3 style={sectionTitleStyle}>排版设置</h3>
        <div style={fieldStyle}>
          <label style={labelStyle}>默认字体</label>
          <select value={store.fontFamily} onChange={(e) => store.setFontFamily(e.target.value)} style={selectStyle}>
            {FONT_OPTIONS.map((f) => (<option key={f.value} value={f.value}>{f.label}</option>))}
          </select>
        </div>
        <div style={fieldStyle}>
          <label style={labelStyle}>字号: {store.fontSize}px</label>
          <input type="range" min={12} max={28} value={store.fontSize}
            onChange={(e) => store.setFontSize(Number(e.target.value))} style={{ flex: 1 }} />
        </div>
        <div style={fieldStyle}>
          <label style={labelStyle}>行高: {store.lineHeight}</label>
          <input type="range" min={1.2} max={2.5} step={0.1} value={store.lineHeight}
            onChange={(e) => store.setLineHeight(Number(e.target.value))} style={{ flex: 1 }} />
        </div>
        <div style={fieldStyle}>
          <label style={labelStyle}>默认预览模式</label>
          <div style={{ display: 'flex', gap: '2px' }}>
            {PREVIEW_MODES.map((m) => (
              <button key={m.value} onClick={() => store.setPreviewMode(m.value)}
                style={{
                  padding: '4px 10px', fontSize: '12px', border: '1px solid var(--border)',
                  background: store.previewMode === m.value ? 'var(--bg-active)' : 'transparent',
                  color: store.previewMode === m.value ? 'var(--text-accent)' : 'var(--text-muted)',
                  cursor: 'pointer', borderRadius: '4px',
                }}>{m.label}</button>
            ))}
          </div>
        </div>
      </div>

      {/* AI Completion */}
      <div style={{ marginBottom: '20px' }}>
        <h3 style={sectionTitleStyle}>AI 补全设置</h3>
        <div style={fieldStyle}>
          <label style={labelStyle}>内联补全</label>
          <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '13px' }}>
            <input type="checkbox" checked={store.inlineCompletionEnabled}
              onChange={(e) => store.setInlineCompletionEnabled(e.target.checked)} />
            启用 Ghost 文本补全
          </label>
        </div>
        <div style={fieldStyle}>
          <label style={labelStyle}>补全延迟 (ms)</label>
          <input type="number" value={store.shortDebounceMs} min={150} max={5000} step={50}
            onChange={(e) => useWriteStore.setState({ shortDebounceMs: Number(e.target.value) })}
            style={inputStyle} />
        </div>
        <div style={fieldStyle}>
          <label style={labelStyle}>长补全延迟 (ms)</label>
          <input type="number" value={store.longDebounceMs} min={1000} max={15000} step={100}
            onChange={(e) => useWriteStore.setState({ longDebounceMs: Number(e.target.value) })}
            style={inputStyle} />
        </div>
      </div>

      {/* Workspace */}
      <div style={{ marginBottom: '20px' }}>
        <h3 style={sectionTitleStyle}>工作区</h3>
        <div style={fieldStyle}>
          <label style={labelStyle}>RAG 检索</label>
          <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '13px' }}>
            <input type="checkbox" checked={store.retrievalEnabled}
              onChange={(e) => store.setRetrievalEnabled(e.target.checked)} />
            启用工作区智能检索
          </label>
        </div>
        <div style={fieldStyle}>
          <label style={labelStyle}>自动保存间隔 (ms)</label>
          <input type="number" value={store.autoSaveIntervalMs} min={300} max={5000} step={100}
            onChange={(e) => useWriteStore.setState({ autoSaveIntervalMs: Number(e.target.value) })}
            style={inputStyle} />
        </div>
      </div>
    </div>
  );
};
