// Template shortcuts — auto-expand @date and other shortcuts during typing

export interface TemplateShortcut {
  trigger: string;
  handler: () => string;
}

export function getDefaultShortcuts(): TemplateShortcut[] {
  return [
    {
      trigger: '@date',
      handler: () => {
        const now = new Date();
        const y = now.getFullYear();
        const m = String(now.getMonth() + 1).padStart(2, '0');
        const d = String(now.getDate()).padStart(2, '0');
        return `${y}-${m}-${d}`;
      },
    },
    {
      trigger: '@time',
      handler: () => {
        const now = new Date();
        const h = String(now.getHours()).padStart(2, '0');
        const m = String(now.getMinutes()).padStart(2, '0');
        return `${h}:${m}`;
      },
    },
    {
      trigger: '@now',
      handler: () => {
        const now = new Date();
        const y = now.getFullYear();
        const mo = String(now.getMonth() + 1).padStart(2, '0');
        const d = String(now.getDate()).padStart(2, '0');
        const h = String(now.getHours()).padStart(2, '0');
        const mi = String(now.getMinutes()).padStart(2, '0');
        return `${y}-${mo}-${d} ${h}:${mi}`;
      },
    },
  ];
}

/**
 * Check if the text before cursor ends with a trigger, and return the expansion.
 * Returns null if no trigger matches.
 */
export function tryExpandShortcut(
  textBeforeCursor: string,
): { trigger: string; expansion: string } | null {
  const shortcuts = getDefaultShortcuts();

  for (const sc of shortcuts) {
    if (textBeforeCursor.endsWith(sc.trigger)) {
      return { trigger: sc.trigger, expansion: sc.handler() };
    }
  }

  return null;
}
