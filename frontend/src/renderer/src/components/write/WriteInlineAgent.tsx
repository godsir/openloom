// Floating selection toolbar — appears when text is selected in the editor
// Provides: block type switching, inline formatting, AI edit input, quick actions

import React, { useState, useRef } from 'react';
import { useWriteStore, WriteBlockType } from '../../stores/write';
import { detectBlockType, applyBlockType } from '../../write/block-type';
import { toggleInlineFormat, hasInlineFormat, InlineFormatKind } from '../../write/inline-format';
import { DEFAULT_QUICK_ACTIONS } from '../../write/quick-actions';
import { resolveAgentPreset } from '../../write/agent-presets';
import { createQuotedSelection } from '../../write/quoted-selection';
import {
  IconHeading1, IconHeading2, IconHeading3,
  IconPilcrow, IconQuote, IconList, IconListOrdered, IconCode2,
  IconBold, IconItalic, IconStrikethrough,
  IconMessageSquare,
  IconSparkles, IconWorkflow, IconPenLine, IconScanSearch, IconClipboardCheck,
} from '../../utils/icons';
import styles from './WriteInlineAgent.module.css';

interface WriteInlineAgentProps {
  editorValue: string;
  onApplyEdit: (newContent: string) => void;
  onSendToAssistant: (text: string) => void;
}

interface BlockTypeDef {
  type: WriteBlockType;
  label: string;
  Icon: React.ComponentType<{ size?: number }>;
}

const BLOCK_TYPES: BlockTypeDef[] = [
  { type: 'paragraph',  label: '正文', Icon: IconPilcrow },
  { type: 'heading1',   label: '标题1', Icon: IconHeading1 },
  { type: 'heading2',   label: '标题2', Icon: IconHeading2 },
  { type: 'heading3',   label: '标题3', Icon: IconHeading3 },
  { type: 'quote',      label: '引用', Icon: IconQuote },
  { type: 'bullet',     label: '无序列表', Icon: IconList },
  { type: 'ordered',    label: '有序列表', Icon: IconListOrdered },
  { type: 'code',       label: '代码块', Icon: IconCode2 },
];

interface InlineFormatDef {
  kind: InlineFormatKind;
  label: string;
  Icon: React.ComponentType<{ size?: number }>;
}

const INLINE_FORMATS: InlineFormatDef[] = [
  { kind: 'bold',          label: '加粗',   Icon: IconBold },
  { kind: 'italic',        label: '斜体',   Icon: IconItalic },
  { kind: 'strikethrough', label: '删除线', Icon: IconStrikethrough },
  { kind: 'code',          label: '行内代码', Icon: IconCode2 },
];

const QUICK_ACTIONS = DEFAULT_QUICK_ACTIONS.slice(0, 5);

const PERSONA_IDS = ['plot-coordinator', 'line-editor', 'foreshadowing', 'continuity'] as const;

const PERSONA_ICON: Record<string, React.ComponentType<{ size?: number }>> = {
  'plot-coordinator': IconWorkflow,
  'line-editor': IconPenLine,
  foreshadowing: IconScanSearch,
  continuity: IconClipboardCheck,
};

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

  const [aiInput, setAiInput] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  const hasSelection = selection !== null;

  const handleBlockType = (type: WriteBlockType) => {
    if (!selection) return;
    const lines = selection.text.split('\n');
    const newLines = applyBlockType(lines, type);
    const newText = newLines.join('\n');
    const before = editorValue.slice(0, selection.from);
    const after = editorValue.slice(selection.to);
    onApplyEdit(before + newText + after);
    setSelection(null);
  };

  const handleInlineFormat = (kind: InlineFormatKind) => {
    if (!selection) return;
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
    const context = selection ? `\n\n${selection.text}` : '';
    if (action.mode === 'chat') {
      onSendToAssistant(`${action.prompt}${context}`);
    } else {
      onSendToAssistant(`${action.prompt}\n\n"${context.trim()}"`);
    }
    setSelection(null);
  };

  const handleQuoteToAssistant = () => {
    if (!selection) return;
    const qs = createQuotedSelection(selection, '');
    addQuotedSelection(qs);
    setSelection(null);
  };

  const handleAiEditSubmit = () => {
    if (!aiInput.trim()) return;
    const context = selection ? `\n\n"${selection.text}"` : '';
    onSendToAssistant(`${aiInput}${context}`);
    setAiInput('');
    setSelection(null);
  };

  return (
    <div className={styles.toolbar}>
      {/* ── Row 1: Block types | Inline formats | Quick actions ── */}
      <div className={styles.row}>
        <div className={styles.section}>
          {BLOCK_TYPES.map((bt) => {
            const currentType = selection ? detectBlockType(selection.text.split('\n')[0]) : null;
            const active = currentType === bt.type;
            const BtnIcon = bt.Icon;
            return (
              <button
                key={bt.type}
                className={active ? styles.iconBtnActive : styles.iconBtn}
                onClick={() => handleBlockType(bt.type)}
                title={bt.label}
                disabled={!hasSelection}
              >
                <BtnIcon size={14} />
              </button>
            );
          })}
        </div>

        <span className={styles.divider} />

        <div className={styles.section}>
          {INLINE_FORMATS.map((f) => {
            const active = hasSelection && hasInlineFormat(selection!.text, f.kind);
            const FmtIcon = f.Icon;
            return (
              <button
                key={f.kind}
                className={active ? styles.iconBtnActive : styles.iconBtn}
                onClick={() => handleInlineFormat(f.kind)}
                title={f.label}
                disabled={!hasSelection}
              >
                <FmtIcon size={14} />
              </button>
            );
          })}
        </div>

        <span className={styles.divider} />

        <div className={styles.section}>
          {QUICK_ACTIONS.map((qa) => (
            <button
              key={qa.id}
              className={styles.quickBtn}
              onClick={() => handleQuickAction(qa.id)}
            >
              {qa.label}
            </button>
          ))}
          <button
            className={styles.btn}
            onClick={handleQuoteToAssistant}
            disabled={!hasSelection}
            title="引用选中内容到助手"
          >
            <IconMessageSquare size={12} />
            <span>引用</span>
          </button>
        </div>
      </div>

      {/* ── Row 2: AI input | Persona switcher ── */}
      <div className={styles.row}>
        <input
          ref={inputRef}
          className={styles.input}
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
          placeholder="AI 指令... (Enter 发送)"
        />

        <span className={styles.divider} />

        <div className={styles.section}>
          <button
            className={!agentPresetId ? styles.presetBtnActive : styles.presetBtn}
            onClick={() => setAgentPresetId(null)}
          >
            <IconSparkles size={13} />
            <span>默认</span>
          </button>
          {PERSONA_IDS.map((id) => {
            const preset = resolveAgentPreset(id);
            const PIcon = PERSONA_ICON[id];
            return (
              <button
                key={id}
                className={agentPresetId === id ? styles.presetBtnActive : styles.presetBtn}
                onClick={() => setAgentPresetId(id)}
                title={preset?.persona}
              >
                <PIcon size={13} />
                <span>{preset?.name}</span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
};
