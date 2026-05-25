/**
 * slash-commands.ts — Loom 斜杠命令定义
 */

import { useStore } from '../../stores';
import { loadSessions } from '../../stores/session-actions';
import { getWebSocket } from '../../services/websocket';
import { loomRpc } from '../../adapter';

// ── Slash Command Interface ──

export interface SlashItem {
  name: string;
  label: string;
  description: string;
  busyLabel: string;
  icon: string;
  type: 'builtin' | 'skill';
  execute: () => Promise<void> | void;
}

export const MAX_SLASH_TRIGGER_LENGTH = 20;

export function getSlashMatches(text: string, commands: SlashItem[]): SlashItem[] {
  const normalized = text.trim();
  if (!normalized.startsWith('/') || normalized.length > MAX_SLASH_TRIGGER_LENGTH) return [];
  const query = normalized.slice(1).toLowerCase();
  return commands.filter(command => command.name.startsWith(query));
}

export function resolveSlashSubmitSelection({
  text,
  skills,
  commands,
  selectedIndex,
  dismissedText,
}: {
  text: string;
  skills: string[];
  commands: SlashItem[];
  selectedIndex: number;
  dismissedText: string | null;
}): SlashItem | null {
  if (skills.length > 0) return null;
  const matches = getSlashMatches(text, commands);
  if (matches.length === 0) return null;
  if (dismissedText === text.trim()) return null;
  return matches[selectedIndex] || matches[0] || null;
}

// ── Loom Commands ──

async function execNewSession(addToast: (text: string) => void) {
  try {
    const result = await loomRpc('session.create', {});
    const sid = result?.session_id || result?.path;
    if (sid) {
      useStore.getState().setCurrentSessionPath(sid);
      await loadSessions();
      addToast('已创建新会话');
    }
  } catch {
    // Also try via WebSocket
    const ws = getWebSocket();
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'prompt', text: '/new', sessionPath: '' }));
    }
  }
}

async function execStop() {
  const sid = useStore.getState().currentSessionPath || '';
  try {
    await loomRpc('chat.abort', { session_id: sid });
  } catch {
    const ws = getWebSocket();
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ jsonrpc: '2.0', method: 'chat.abort', params: { session_id: sid }, id: Date.now() }));
    }
  }
}

async function execClear(addToast: (text: string) => void) {
  try {
    const result = await loomRpc('session.create', {});
    const sid = result?.session_id || result?.path;
    if (sid) {
      useStore.getState().setCurrentSessionPath(sid);
      await loadSessions();
      addToast('已创建新会话，上下文已清空');
    }
  } catch {
    addToast('创建新会话失败');
  }
}

async function execCompact(addToast: (text: string, type?: string) => void) {
  const sid = useStore.getState().currentSessionPath || '';
  if (!sid) return;
  addToast('压缩中...', 'info');
  try {
    await loomRpc('session.compact', { session_id: sid });
    addToast('上下文已压缩', 'success');
  } catch {
    addToast('压缩失败', 'error');
  }
}

export function buildSlashCommands(
  t: (key: string) => string,
  addToast: (text: string, type?: string) => void,
): SlashItem[] {
  return [
    {
      name: 'new',
      label: '/new',
      description: t('slash.new'),
      busyLabel: '',
      icon: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14M5 12h14"/></svg>',
      type: 'builtin',
      execute: () => execNewSession(addToast),
    },
    {
      name: 'compact',
      label: '/compact',
      description: t('slash.compact'),
      busyLabel: t('slash.compactBusy'),
      icon: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 14 10 14 10 20"/><polyline points="20 10 14 10 14 4"/><line x1="14" y1="10" x2="21" y2="3"/><line x1="3" y1="21" x2="10" y2="14"/></svg>',
      type: 'builtin',
      execute: () => execCompact(addToast),
    },
    {
      name: 'stop',
      label: '/stop',
      description: t('slash.stop'),
      busyLabel: '',
      icon: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="6" width="12" height="12" rx="1"/></svg>',
      type: 'builtin',
      execute: execStop,
    },
    {
      name: 'clear',
      label: '/clear',
      description: t('slash.clear'),
      busyLabel: '',
      icon: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/></svg>',
      type: 'builtin',
      execute: () => execClear(addToast),
    },
  ];
}
