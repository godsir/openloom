// Recent edits tracking — maintains a FIFO buffer of recent AI edits
// Provides context for subsequent inline edits to maintain consistency

import type { RecentEdit } from '../stores/write';

const MAX_RECENT_EDITS = 48;
const RECENT_EDIT_TTL_MS = 120_000; // 2 minutes
const RECENT_EDIT_TEXT_LIMIT = 200; // characters per edit

export interface RecentEditInput {
  instruction: string;
  originalText: string;
  editedText: string;
  filePath: string;
}

export function createRecentEdit(input: RecentEditInput): RecentEdit {
  return {
    instruction: input.instruction.slice(0, 100),
    originalText: input.originalText.slice(0, RECENT_EDIT_TEXT_LIMIT),
    editedText: input.editedText.slice(0, RECENT_EDIT_TEXT_LIMIT),
    filePath: input.filePath,
    timestamp: Date.now(),
  };
}

export function trimRecentEdits(edits: RecentEdit[], now: number = Date.now()): RecentEdit[] {
  // Filter by TTL
  const cutoff = now - RECENT_EDIT_TTL_MS;
  const live = edits.filter((e) => e.timestamp > cutoff);
  // Keep max
  return live.slice(0, MAX_RECENT_EDITS);
}

/**
 * Format recent edits for inclusion in an AI prompt.
 * Returns the 8 most recent edits, formatted as context lines.
 */
export function formatRecentEditsForPrompt(
  edits: RecentEdit[],
  currentFilePath: string,
): string[] {
  const relevant = edits
    .filter((e) => e.filePath === currentFilePath)
    .slice(0, 8);

  return relevant.map(
    (e) => `[Edit] "${e.instruction}": "${e.originalText}" → "${e.editedText}"`,
  );
}
