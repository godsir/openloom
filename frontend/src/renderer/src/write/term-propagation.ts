// Term propagation — detects renamed terms and propagates changes to nearby occurrences

const PROPAGATION_WINDOW = 6000; // chars to search around the edit
const MAX_PROPAGATIONS = 16; // max replacements per edit

export interface TermChange {
  oldTerm: string;
  newTerm: string;
  positions: number[]; // char offsets where oldTerm was replaced
}

export function detectTermChange(
  originalText: string,
  modifiedText: string,
): TermChange | null {
  if (!originalText || !modifiedText) return null;
  if (originalText === modifiedText) return null;

  // Simple approach: find the first differing word
  const origWords = originalText.split(/\b/);
  const modWords = modifiedText.split(/\b/);

  for (let i = 0; i < Math.min(origWords.length, modWords.length); i++) {
    if (origWords[i] !== modWords[i]) {
      const oldTerm = origWords[i].trim();
      const newTerm = modWords[i].trim();
      if (oldTerm && newTerm && oldTerm.length > 1 && newTerm.length > 1) {
        return { oldTerm, newTerm, positions: [] };
      }
      break;
    }
  }

  return null;
}

export function propagateTerm(
  content: string,
  change: TermChange,
  scopeStart: number,
  scopeEnd: number,
): string | null {
  const scope = content.slice(
    Math.max(0, scopeStart - PROPAGATION_WINDOW),
    Math.min(content.length, scopeEnd + PROPAGATION_WINDOW),
  );
  const scopeOffset = Math.max(0, scopeStart - PROPAGATION_WINDOW);

  // Find occurrences of oldTerm in scope (excluding the original position)
  const escaped = change.oldTerm.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const regex = new RegExp(`\\b${escaped}\\b`, 'g');
  let match;
  let count = 0;

  // Build replacement
  const replacements: { from: number; to: number; term: string }[] = [];
  while ((match = regex.exec(scope)) !== null && count < MAX_PROPAGATIONS) {
    const absPos = scopeOffset + match.index;
    // Skip the original change position
    if (absPos >= scopeStart && absPos <= scopeEnd) continue;
    replacements.push({ from: absPos, to: absPos + change.oldTerm.length, term: change.newTerm });
    count++;
  }

  if (replacements.length === 0) return null;

  // Apply replacements from end to start to preserve offsets
  let result = content;
  for (const r of replacements.reverse()) {
    result = result.slice(0, r.from) + r.term + result.slice(r.to);
    change.positions.push(r.from);
  }

  return result;
}
