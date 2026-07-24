// Inline Edit 编排服务：把 inline-edit.ts 的纯函数管道接到真实 LLM 调用上。
// 流程：选区 → 构造 prompt → completion.chat（一次性非流式）→ 解析 <<<EDIT 标记
// → 生成 DiffChunk 进入待审查态（WriteReviewBar 展示，用户接受/拒绝）。

import { loomRpc } from '../services/jsonrpc';
import { useWriteStore } from '../stores/write';
import { t } from '../i18n';
import {
  resolveEditScope,
  buildInlineEditPrompt,
  parseInlineEditResponse,
  buildDiffChunks,
  type WriteInlineEditScope,
} from './inline-edit';

interface CompletionChatResult {
  ok: boolean;
  content?: string;
  message?: string;
}

// 当前待审查编辑的上下文（单例：同一时间只允许一个待审查编辑）
let pendingInstruction: string | null = null;
let pendingScope: WriteInlineEditScope | null = null;

/** 请求 AI 对当前选区执行 inline 编辑。结果进入待审查态，不直接改文档。 */
export async function requestInlineEdit(
  instruction: string,
): Promise<{ ok: boolean; message?: string }> {
  const ws = useWriteStore.getState();
  if (ws.reviewActive) {
    return { ok: false, message: t('write.inlineEditReviewPending') };
  }
  const selection = ws.selection;
  const filePath = ws.activeFilePath;
  if (!selection || !filePath) {
    return { ok: false, message: t('write.inlineEditNoSelection') };
  }
  // TipTap 的 from/to 是 ProseMirror 坐标，与 markdown 文本偏移不兼容。
  // 调用方在 rich 模式下应已回退到聊天路径，这里是双保险。
  if (selection.source !== 'markdown') {
    return { ok: false, message: t('write.inlineEditRichUnsupported') };
  }
  const content = ws.fileContent;

  const scope = resolveEditScope(content, selection, 'selection');
  if (!scope.text.trim()) {
    return { ok: false, message: t('write.inlineEditNoSelection') };
  }

  const prompt = buildInlineEditPrompt({
    prefix: scope.prefix,
    editScope: scope.text,
    suffix: scope.suffix,
    instruction,
    filePath,
    recentEdits: ws.recentEdits
      .slice(0, 5)
      .map((e) => `${e.instruction} -> ${e.editedText.slice(0, 80)}`),
  });

  // max_tokens 随选区长度自适应（替换文本通常与原选区同量级），上限 4096
  const maxTokens = Math.min(4096, Math.max(1024, Math.ceil(scope.text.length * 1.5)));

  let result: CompletionChatResult;
  try {
    result = await loomRpc<CompletionChatResult>('completion.chat', {
      messages: [
        {
          role: 'system',
          content:
            '你是严谨的文本编辑助手。只修改 EDIT_SCOPE 内的文字，严格保持上下文连贯。' +
            '只输出 <<<EDIT 和 <<</EDIT 标记包裹的修改后文本，不要输出任何其他内容。',
        },
        { role: 'user', content: prompt },
      ],
      max_tokens: maxTokens,
      temperature: 0.3,
      // 优先使用写作模式的模型选择，未设置时后端回退到全局活跃模型
      model: ws.writingModelName || undefined,
    });
  } catch (e: any) {
    return { ok: false, message: e?.message || String(e) };
  }

  if (!result?.ok) {
    return { ok: false, message: result?.message || t('write.inlineEditFailed') };
  }

  const replacement = parseInlineEditResponse(result.content ?? '');
  if (replacement === null) {
    return { ok: false, message: t('write.inlineEditParseFailed') };
  }

  pendingInstruction = instruction;
  pendingScope = scope;
  ws.setPendingAgentReview(buildDiffChunks(content, scope, replacement));
  ws.setReviewActive(true);
  return { ok: true };
}

/** 接受待审查的编辑：应用到文档、记录 recentEdit、清除审查态。 */
export function acceptInlineEdit(): { ok: boolean; message?: string } {
  const ws = useWriteStore.getState();
  const chunks = ws.pendingAgentReview;
  if (!ws.reviewActive || !chunks || chunks.length === 0) return { ok: false };

  // 逐块校验偏移未失效（用户在等待期间又改了文档会导致偏移错位）
  let content = ws.fileContent;
  // 从后往前应用，避免前面的替换改变后面的偏移
  const ordered = [...chunks].sort((a, b) => b.fromA - a.fromA);
  for (const c of ordered) {
    if (content.slice(c.fromA, c.toA) !== c.originalText) {
      ws.setPendingAgentReview(null);
      ws.setReviewActive(false);
      pendingInstruction = null;
      pendingScope = null;
      return { ok: false, message: t('write.inlineEditStale') };
    }
    content = content.slice(0, c.fromA) + c.modifiedText + content.slice(c.toA);
  }

  ws.setFileContent(content);
  ws.setSaveStatus('dirty');
  if (pendingInstruction && pendingScope) {
    ws.addRecentEdit({
      instruction: pendingInstruction,
      originalText: pendingScope.text,
      editedText: chunks[0].modifiedText,
      filePath: ws.activeFilePath || '',
      timestamp: Date.now(),
    });
  }
  ws.setPendingAgentReview(null);
  ws.setReviewActive(false);
  ws.setSelection(null);
  pendingInstruction = null;
  pendingScope = null;
  return { ok: true };
}

/** 拒绝待审查的编辑：丢弃，不改文档。 */
export function rejectInlineEdit(): void {
  const ws = useWriteStore.getState();
  ws.setPendingAgentReview(null);
  ws.setReviewActive(false);
  pendingInstruction = null;
  pendingScope = null;
}
