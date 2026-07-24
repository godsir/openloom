// Floating selection toolbar — appears when text is selected in the editor
// Provides: block type switching, inline formatting, quick actions, quote to assistant
// AI input lives in WriteAssistantPanel (right side) — not duplicated here.

import React, { useState } from 'react';
import { useWriteStore } from '../../stores/write';
import { detectBlockType, applyBlockType, type WriteBlockType } from '../../write/block-type';
import { toggleInlineFormat, hasInlineFormat, InlineFormatKind } from '../../write/inline-format';
import { DEFAULT_QUICK_ACTIONS } from '../../write/quick-actions';
import { createQuotedSelection } from '../../write/quoted-selection';
import { requestInlineEdit } from '../../write/inline-edit-service';
import { WriteReviewBar } from './WriteReviewBar';
import { useLocale } from '../../i18n';
import {
  IconHeading1, IconHeading2, IconHeading3,
  IconPilcrow, IconQuote, IconList, IconListOrdered, IconCode2,
  IconBold, IconItalic, IconStrikethrough,
  IconMessageSquare, IconRotateCcw, IconRotateCw,
} from '../../utils/icons';
import styles from './WriteInlineAgent.module.css';
import type { RichEditorActiveState } from '../../write/tiptap/WriteRichEditor';

interface WriteInlineAgentProps {
  editorValue: string;
  onApplyEdit: (newContent: string) => void;
  onSendToAssistant: (text: string) => void;
  onRichBlockType?: (type: WriteBlockType) => void;
  onRichInlineFormat?: (kind: InlineFormatKind) => void;
  getRichActiveState?: () => RichEditorActiveState | undefined;
  onUndo?: () => void;
  onRedo?: () => void;
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

export const WriteInlineAgent: React.FC<WriteInlineAgentProps> = ({
  editorValue,
  onApplyEdit,
  onSendToAssistant,
  onRichBlockType,
  onRichInlineFormat,
  getRichActiveState,
  onUndo,
  onRedo,
}) => {
  const { t } = useLocale();
  const selection = useWriteStore((s) => s.selection);
  const setSelection = useWriteStore((s) => s.setSelection);
  const addQuotedSelection = useWriteStore((s) => s.addQuotedSelection);
  const activeFilePath = useWriteStore((s) => s.activeFilePath);
  // 进行中的 inline 编辑请求（action id），防止并发触发
  const [inlineBusy, setInlineBusy] = useState<string | null>(null);

  const hasSelection = selection !== null;
  const richState = selection?.source === 'rich' ? getRichActiveState?.() : undefined;

  const handleBlockType = (type: WriteBlockType) => {
    if (!selection) return;
    if (selection.source === 'rich') {
      onRichBlockType?.(type);
      return;
    }
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
    if (selection.source === 'rich') {
      onRichInlineFormat?.(kind);
      return;
    }
    const formatted = toggleInlineFormat(selection.text, kind);
    if (formatted === null) return;
    const before = editorValue.slice(0, selection.from);
    const after = editorValue.slice(selection.to);
    onApplyEdit(before + formatted + after);
    setSelection(null);
  };

  const handleQuickAction = async (actionId: string) => {
    const action = DEFAULT_QUICK_ACTIONS.find((a) => a.id === actionId);
    if (!action) return;
    const context = selection ? `\n\n${selection.text}` : '';
    if (action.mode === 'chat') {
      onSendToAssistant(`${action.prompt}${context}`);
      setSelection(null);
      return;
    }
    // inline 模式：AI 直接改写选区，结果进入审查栏（不直接落地）。
    if (!selection) return;
    // TipTap 的选区坐标是 ProseMirror 位置，与 markdown 文本偏移不兼容——
    // rich 模式下回退到聊天路径（行为与改动前一致）。
    if (selection.source === 'rich') {
      onSendToAssistant(`${action.prompt}\n\n"${context.trim()}"`);
      setSelection(null);
      return;
    }
    if (inlineBusy) return;
    setInlineBusy(actionId);
    try {
      const result = await requestInlineEdit(action.prompt);
      if (!result.ok) {
        useWriteStore.getState().showToast(
          'error',
          result.message || t('write.inlineEditFailed'),
        );
      }
    } finally {
      setInlineBusy(null);
    }
  };

  const handleQuoteToAssistant = () => {
    if (!selection) return;
    const qs = createQuotedSelection(selection, activeFilePath || '');
    addQuotedSelection(qs);
    setSelection(null);
  };

  return (
    <div className={styles.toolbar}>
      {/* Block types | Inline formats | Quick actions */}
      <div className={styles.row}>
        <div className={styles.section}>
          <button
            className={styles.iconBtn}
            onClick={onUndo}
            title="撤销 (Ctrl+Z)"
            disabled={!onUndo}
          >
            <IconRotateCcw size={14} />
          </button>
          <button
            className={styles.iconBtn}
            onClick={onRedo}
            title="重做 (Ctrl+Shift+Z)"
            disabled={!onRedo}
          >
            <IconRotateCw size={14} />
          </button>
        </div>

        <span className={styles.divider} />

        <div className={styles.section}>
          {BLOCK_TYPES.map((bt) => {
            const currentType = selection?.source === 'rich'
              ? richState?.block
              : selection ? detectBlockType(selection.text.split('\n')[0]) : null;
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
            const active = selection?.source === 'rich'
              ? !!richState?.[f.kind]
              : hasSelection && hasInlineFormat(selection!.text, f.kind);
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
              disabled={!hasSelection || inlineBusy !== null}
              title={
                !hasSelection
                  ? '请先选中文本'
                  : qa.mode === 'inline'
                    ? 'AI 直接改写选区，结果需确认后落地'
                    : '发送到右侧助手处理'
              }
            >
              {inlineBusy === qa.id ? '…' : qa.label}
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
      {/* AI inline 编辑的审查栏（有待确认修改时可见） */}
      <WriteReviewBar />
    </div>
  );
};
