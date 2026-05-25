import { useState, useEffect } from 'react';
import { useStore } from '../../stores';
import { loomRpc } from '../../adapter';
import type { BridgeSession } from '../../stores/bridge-slice';

const PLATFORM_ICONS: Record<string, string> = {
  telegram: '✈️',
  feishu: '🐦',
  wechat: '💬',
  qq: '🐧',
};

export function BridgeSessionList({ onSelect }: { onSelect: (sessionId: string) => void }) {
  const { bridgeSessions, setBridgeSessions, bridgeActiveSession } = useStore();
  const [filter, setFilter] = useState<string>('all');

  useEffect(() => {
    loomRpc('bridge.sessions', { platform: filter === 'all' ? undefined : filter })
      .then((res: any) => {
        setBridgeSessions(res?.sessions || []);
      })
      .catch(() => {});
  }, [filter, setBridgeSessions]);

  const deleteSession = async (sessionId: string) => {
    try {
      await loomRpc('bridge.session.delete', { session_id: sessionId });
      setBridgeSessions(bridgeSessions.filter(s => s.id !== sessionId));
    } catch {}
  };

  return (
    <div className="bridge-session-list">
      <div className="bridge-filter" style={{ display: 'flex', gap: 4, marginBottom: 8 }}>
        {['all', 'telegram', 'feishu', 'wechat', 'qq'].map(p => (
          <button
            key={p}
            onClick={() => setFilter(p)}
            style={{
              padding: '2px 8px',
              borderRadius: 4,
              border: '1px solid #ccc',
              background: filter === p ? '#4a9' : 'transparent',
              cursor: 'pointer',
              fontSize: 12,
            }}
          >
            {p === 'all' ? 'All' : `${PLATFORM_ICONS[p] || ''} ${p}`}
          </button>
        ))}
      </div>
      {bridgeSessions.length === 0 && (
        <div style={{ color: '#888', padding: 16, textAlign: 'center' }}>No bridge sessions</div>
      )}
      {bridgeSessions.map(s => (
        <div
          key={s.id}
          onClick={() => onSelect(s.id)}
          style={{
            padding: '8px 12px',
            cursor: 'pointer',
            borderRadius: 6,
            marginBottom: 4,
            background: bridgeActiveSession === s.id ? '#3a7' : 'transparent',
            display: 'flex',
            alignItems: 'center',
            gap: 8,
          }}
        >
          <span>{PLATFORM_ICONS[s.platform] || '📱'}</span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {s.userName || s.chatId}
            </div>
            <div style={{ fontSize: 11, color: '#888' }}>
              {s.platform} · {s.messageCount} msgs
            </div>
          </div>
          <button
            onClick={(e) => { e.stopPropagation(); deleteSession(s.id); }}
            style={{ background: 'none', border: 'none', cursor: 'pointer', color: '#c55', fontSize: 14 }}
            title="Delete"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
