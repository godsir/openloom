import { useEffect, useState } from 'react';
import { useStore } from '../../stores';
import { loomRpc } from '../../adapter';
import type { BridgeMessage } from '../../stores/bridge-slice';

export function BridgeMessageViewer({ sessionId }: { sessionId: string }) {
  const { bridgeMessages, setBridgeMessages } = useStore();
  const [reply, setReply] = useState('');

  useEffect(() => {
    loomRpc('bridge.messages', { session_id: sessionId, limit: 50, offset: 0 })
      .then((res: any) => {
        setBridgeMessages(res?.messages || []);
      })
      .catch(() => {});
  }, [sessionId, setBridgeMessages]);

  const sendReply = async () => {
    if (!reply.trim()) return;
    const session = useStore.getState().bridgeSessions.find(s => s.id === sessionId);
    if (!session) return;
    try {
      await loomRpc('bridge.send', {
        platform: session.platform,
        chat_id: session.chatId,
        text: reply.trim(),
      });
      setReply('');
      // Reload messages
      const res = await loomRpc('bridge.messages', { session_id: sessionId, limit: 50, offset: 0 });
      setBridgeMessages((res as any)?.messages || []);
    } catch {}
  };

  return (
    <div className="bridge-message-viewer" style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ flex: 1, overflowY: 'auto', padding: 12 }}>
        {bridgeMessages.length === 0 && (
          <div style={{ color: '#888', textAlign: 'center', padding: 24 }}>No messages yet</div>
        )}
        {[...bridgeMessages].reverse().map((m: BridgeMessage) => (
          <div
            key={m.id}
            style={{
              display: 'flex',
              justifyContent: m.direction === 'inbound' ? 'flex-start' : 'flex-end',
              marginBottom: 8,
            }}
          >
            <div
              style={{
                maxWidth: '75%',
                padding: '8px 12px',
                borderRadius: 12,
                background: m.direction === 'inbound' ? '#e8e8e8' : '#4a9',
                color: m.direction === 'inbound' ? '#333' : '#fff',
              }}
            >
              {m.mediaType !== 'text' && (
                <div style={{ fontSize: 11, opacity: 0.7, marginBottom: 4 }}>
                  [{m.mediaType}]
                  {m.mediaUrl && <span> {m.mediaUrl.slice(0, 30)}...</span>}
                </div>
              )}
              {m.content && <div>{m.content}</div>}
              <div style={{ fontSize: 10, opacity: 0.5, marginTop: 4 }}>
                {new Date(m.timestamp).toLocaleTimeString()}
              </div>
            </div>
          </div>
        ))}
      </div>
      <div style={{ padding: '8px 12px', borderTop: '1px solid #ddd', display: 'flex', gap: 8 }}>
        <input
          value={reply}
          onChange={e => setReply(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') sendReply(); }}
          placeholder="Type a reply..."
          style={{ flex: 1, padding: '8px 12px', borderRadius: 6, border: '1px solid #ccc' }}
        />
        <button
          onClick={sendReply}
          style={{ padding: '8px 16px', borderRadius: 6, background: '#4a9', color: '#fff', border: 'none', cursor: 'pointer' }}
        >
          Send
        </button>
      </div>
    </div>
  );
}
