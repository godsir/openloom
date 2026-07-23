// Selection quoting — formats selected text as context blocks for AI assistant
// Tags: [引用原文]...[/引用原文], [相关文献上下文]...[/相关文献上下文]

import type { QuotedSelection, WriteEditorSelectionState } from '../stores/write';

export function createQuotedSelection(
  selection: WriteEditorSelectionState,
  filePath: string,
): QuotedSelection {
  return {
    id: globalThis.crypto?.randomUUID?.() ?? `qs-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    text: selection.text,
    filePath,
    lineFrom: selection.lineFrom,
    lineTo: selection.lineTo,
    timestamp: Date.now(),
  };
}

/**
 * Format a single quoted selection for the AI prompt.
 */
export function formatQuotedSelectionForPrompt(qs: QuotedSelection): string {
  const header = `[引用原文] 来源: ${qs.filePath} (行 ${qs.lineFrom + 1}-${qs.lineTo + 1})`;
  return `${header}\n${qs.text}\n[/引用原文]`;
}

/**
 * Compose the full write prompt including quoted selections and context.
 */
export function composeWritePrompt(
  userInput: string,
  currentFile: string,
  fileContent: string,
  quotedSelections?: QuotedSelection[],
  retrievalContext?: string,
  agentPersona?: string,
): string {
  const parts: string[] = [];

  // Agent persona
  if (agentPersona) {
    parts.push(`[写作人格]\n${agentPersona}\n[/写作人格]\n`);
  }

  // Writing context
  parts.push(`[写作上下文]`);
  parts.push(`当前文件: ${currentFile}`);
  parts.push(`\n${fileContent}`);
  parts.push(`[/写作上下文]\n`);

  // Quoted selections
  if (quotedSelections && quotedSelections.length > 0) {
    parts.push(`[引用片段]`);
    for (const qs of quotedSelections) {
      parts.push(formatQuotedSelectionForPrompt(qs));
    }
    parts.push(`[/引用片段]\n`);
  }

  // Retrieval context
  if (retrievalContext) {
    parts.push(`[相关文献上下文]`);
    parts.push(retrievalContext);
    parts.push(`[/相关文献上下文]\n`);
  }

  // User instruction
  parts.push(`[用户指令]`);
  parts.push(userInput);

  return parts.join('\n');
}

const WRITE_CONTEXT_MAX_CHARS = 20_000;

export function limitWriteContext(content: string): string {
  if (content.length <= WRITE_CONTEXT_MAX_CHARS) return content;
  const head = content.slice(0, 12_000);
  const tail = content.slice(-8_000);
  return `${head}\n\n[正文中间部分因上下文限制已省略]\n\n${tail}`;
}

/**
 * Parse a write prompt for display in the chat UI.
 * Extracts quoted selections, context, and user input.
 */
export function parseWritePromptForDisplay(prompt: string): {
  userInput: string;
  context?: string;
  quotes?: string[];
  retrieval?: string;
} {
  const result: { userInput: string; context?: string; quotes?: string[]; retrieval?: string } = {
    userInput: prompt,
  };

  // Extract context
  const ctxMatch = prompt.match(/\[写作上下文\]([\s\S]*?)\[\/写作上下文\]/);
  if (ctxMatch) {
    result.context = ctxMatch[1].trim();
    result.userInput = result.userInput.replace(ctxMatch[0], '').trim();
  }

  // Extract quotes
  const quoteRegex = /\[引用原文\]([\s\S]*?)\[\/引用原文\]/g;
  const quotes: string[] = [];
  let qm: RegExpExecArray | null;
  while ((qm = quoteRegex.exec(prompt)) !== null) {
    quotes.push(qm[1].trim());
  }
  if (quotes.length > 0) {
    result.quotes = quotes;
    result.userInput = result.userInput.replace(/\[引用片段\][\s\S]*?\[\/引用片段\]/g, '').trim();
  }

  // Extract retrieval context
  const rm = prompt.match(/\[相关文献上下文\]([\s\S]*?)\[\/相关文献上下文\]/);
  if (rm) {
    result.retrieval = rm[1].trim();
    result.userInput = result.userInput.replace(rm[0], '').trim();
  }

  // Extract user instruction
  const ui = result.userInput.match(/\[用户指令\]\s*([\s\S]*)/);
  if (ui) {
    result.userInput = ui[1].trim();
  }

  return result;
}
