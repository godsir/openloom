// Floating selection toolbar — appears when text is selected in the editor
// Provides: block type switching, inline formatting, AI edit input, quick actions

import React, { useState, useRef, useLayoutEffect, useCallback } from 'react';
import { useWriteStore, WriteBlockType } from '../../stores/write';
import { detectBlockType, applyBlockType } from '../../write/block-type';
import { toggleInlineFormat, hasInlineFormat, InlineFormatKind } from '../../write/inline-format';
import { DEFAULT_QUICK_ACTIONS } from '../../write/quick-actions';
import { resolveAgentPreset } from '../../write/agent-presets';
import { createQuotedSelection } from '../../write/quoted-selection';

interface WriteInlineAgentProps {
  editorValue: string;
  onApplyEdit: (newContent: string) => void;
  onSendToAssistant: (text: string) => void;
}

const BLOCK_TYPES: { type: WriteBlockType; label: string }[] = [
  { type: 'paragraph', label: '¶' },
  { type: 'heading1', label: 'H1' },
  { type: 'heading2', label: 'H2' },
  { type: 'heading3', label: 'H3' },
  { type: 'quote', label: '"' },
  { type: 'bullet', label: '•' },
  { type: 'ordered', label: '1.' },
  { type: 'code', label: '<>' },
];

const INLINE_FORMATS: { kind: InlineFormatKind; label: string; icon: string }[] = [
  { kind: 'bold', label: 'B', icon: 'B' },
  { kind: 'italic', label: 'I', icon: 'I' },
  { kind: 'strikethrough', label: 'S', icon: 'S' },
  { kind: 'code', label: '`', icon: '`' },
];

export const WriteInlineAgent: React.FC<WriteInlineAgentProps> = ({
  editorValue,
  onApplyEdit,
  onSendToAssistant,
}) => {
  const selection = useWriteStore((s) => s.selection);
  const setSelection = useWriteStore((s) => s.setSelection);
  const addQuotedSelection = useWriteStore((s) => s.addQuotedSelection);
  const agentPresetId = useWriteStore((s) => s.agentPresetId);
  const setAgentPresetId = useWriteStore((s) => s.setAgentPresetId);
  const fileThreads = useWriteStore((s) => s.fileThreads);

  const [aiInput, setAiInput] = useState('');
  const toolbarRef = useRef<HTMLDivElement>(null);

  const [toolbarPos, setToolbarPos] = useState<{ x: number; y: number } | null>(null);
  const toolbarRef = useRef<HTMLDivElement>(null);

  const activePreset = resolveAgentPreset(agentPresetId);

  // Position toolbar at DOM selection
  useLayoutEffect(() => {
    if (!selection) { setToolbarPos(null); return; }
    const domSel = window.getSelection();
    if (!domSel || domSel.isCollapsed || domSel.rangeCount === 0) { setToolbarPos(null); return; }
    const range = domSel.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    if (!rect || rect.width === 0 || rect.height === 0) { setToolbarPos(null); return; }
    // Position above selection, centered
    const x = Math.max(8, rect.left + rect.width / 2 - 150); // center toolbar (300px wide)
    const y = rect.top - 10; // above selection
    setToolbarPos({ x, y });
  }, [selection]);

  if (!selection || !toolbarPos) return null;

  const handleBlockType = (type: WriteBlockType) => {
    const lines = selection.text.split('\n');
    const newLines = applyBlockType(lines, type);
    const newText = newLines.join('\n');
    const before = editorValue.slice(0, selection.from);
    const after = editorValue.slice(selection.to);
    onApplyEdit(before + newText + after);
    setSelection(null);
  };

  const handleInlineFormat = (kind: InlineFormatKind) => {
    const formatted = toggleInlineFormat(selection.text, kind);
    if (formatted === null) return;
    const before = editorValue.slice(0, selection.from);
    const after = editorValue.slice(selection.to);
    onApplyEdit(before + formatted + after);
    setSelection(null);
  };

  const handleQuickAction = (actionId: string) => {
    const action = DEFAULT_QUICK_ACTIONS.find((a) => a.id === actionId);
    if (!action) return;
    if (action.mode === 'chat') {
      onSendToAssistant(`${action.prompt}\n\n${selection.text}`);
    } else {
      // Inline mode: send to AI edit
      onSendToAssistant(`${action.prompt}\n\n"${selection.text}"`);
    }
    setSelection(null);
  };

  const handleQuoteToAssistant = () => {
    const qs = createQuotedSelection(selection, '');
    addQuotedSelection(qs);
    setSelection(null);
  };

  const handleAiEditSubmit = () => {
    if (!aiInput.trim()) return;
    onSendToAssistant(`${aiInput}\n\n"${selection.text}"`);
    setAiInput('');
    setSelection(null);
  };

  const styles: Record<string, React.CSSProperties> = {
    toolbar: {
      position: 'fixed',
      zIndex: 1000,
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: '8px',
      padding: '6px',
      boxShadow: '0 4px 16px rgba(0,0,0,0.2)',
      display: 'flex',
      flexDirection: 'column',
      gap: '4px',
      minWidth: '300px',
    },
    row: {
      display: 'flex',
      gap: '2px',
      alignItems: 'center',
      flexWrap: 'wrap',
    },
    btn: {
      padding: '3px 7px',
      fontSize: '11px',
      border: '1px solid var(--border)',
      borderRadius: '3px',
      background: 'transparent',
      color: 'var(--text)',
      cursor: 'pointer',
      whiteSpace: 'nowrap',
    },
    btnActive: {
      padding: '3px 7px',
      fontSize: '11px',
      border: '1px solid var(--border)',
      borderRadius: '3px',
      background: 'var(--bg-active)',
      color: 'var(--text-accent)',
      cursor: 'pointer',
      fontWeight: 600,
      whiteSpace: 'nowrap',
    },
    input: {
      flex: 1,
      padding: '4px 8px',
      fontSize: '12px',
      border: '1px solid var(--border)',
      borderRadius: '3px',
      background: 'var(--bg)',
      color: 'var(--text)',
      minWidth: '120px',
    },
    divider: {
      width: '1px',
      height: '16px',
      background: 'var(--border)',
      margin: '0 4px',
    },
    presetBtn: {
      padding: '2px 8px',
      fontSize: '10px',
      border: '1px solid var(--border)',
      borderRadius: '10px',
      background: 'transparent',
      color: 'var(--text-muted)',
      cursor: 'pointer',
    },
    presetBtnActive: {
      padding: '2px 8px',
      fontSize: '10px',
      border: '1px solid var(--text-accent)',
      borderRadius: '10px',
      background: 'var(--bg-active)',
      color: 'var(--text-accent)',
      cursor: 'pointer',
    },
  };

  return (
    <div ref={toolbarRef} style={{ ...styles.toolbar, left: toolbarPos.x, top: toolbarPos.y }}>
      {/* Row 1: Block types */}
      <div style={styles.row}>
        {BLOCK_TYPES.map((bt) => {
          const currentType = detectBlockType(selection.text.split('\n')[0]);
          const active = currentType === bt.type;
          return (
            <button
              key={bt.type}
              style={active ? styles.btnActive : styles.btn}
              onClick={() => handleBlockType(bt.type)}
              title={bt.type}
            >
              {bt.label}
            </button>
          );
        })}
        <span style={styles.divider} />
        {/* Inline formats */}
        {INLINE_FORMATS.map((f) => {
          const active = hasInlineFormat(selection.text, f.kind);
          return (
            <button
              key={f.kind}
              style={active ? styles.btnActive : styles.btn}
              onClick={() => handleInlineFormat(f.kind)}
              title={f.kind}
            >
              {f.icon}
            </button>
          );
        })}
      </div>

      {/* Row 2: Quick actions */}
      <div style={styles.row}>
        {DEFAULT_QUICK_ACTIONS.slice(0, 5).map((qa) => (
          <button
            key={qa.id}
            style={styles.btn}
            onClick={() => handleQuickAction(qa.id)}
          >
            {qa.label}
          </button>
        ))}
        <button style={styles.btn} onClick={handleQuoteToAssistant}>
          💬 引用
        </button>
      </div>

      {/* Row 3: Agent persona switcher */}
      <div style={styles.row}>
        <button
          style={!agentPresetId ? styles.presetBtnActive : styles.presetBtn}
          onClick={() => setAgentPresetId(null)}
        >
          默认
        </button>
        {['plot-coordinator', 'line-editor', 'foreshadowing', 'continuity'].map(
          (id) => {
            const preset = resolveAgentPreset(id);
            return (
              <button
                key={id}
                style={agentPresetId === id ? styles.presetBtnActive : styles.presetBtn}
                onClick={() => setAgentPresetId(id)}
                title={preset?.persona}
              >
                {preset?.emoji} {preset?.name}
              </button>
            );
          },
        )}
      </div>

      {/* Row 4: AI edit input */}
      <div style={styles.row}>
        <input
          style={styles.input}
          value={aiInput}
          onChange={(e) => setAiInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) {
              e.preventDefault();
              handleAiEditSubmit();
            }
            if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              if (aiInput.trim()) {
                onSendToAssistant(aiInput);
                setAiInput('');
                setSelection(null);
              }
            }
          }}
          placeholder="AI 编辑指令... (Enter 发送, Ctrl+Enter 到助手)"
        />
      </div>
    </div>
  );
};
