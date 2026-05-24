import { useState, useEffect, useCallback } from 'react';
import { useSettingsStore } from '../store';
import { useStore } from '../../stores';
import { loomRpc } from '../../adapter';
import { t } from '../helpers';
import { Overlay } from '../../ui';
import styles from '../Settings.module.css';

interface Cognition {
  id: number;
  trait: string;
  value: string;
  confidence: number;
  evidence_count: number;
  version: number;
}

const PAGE_SIZE = 5;

function confidenceBar(pct: number): string {
  const clamped = Math.max(0, Math.min(1, pct));
  const filled = Math.round(clamped * 20);
  return '█'.repeat(filled) + '░'.repeat(20 - filled);
}

export function MemoryViewer() {
  const [visible, setVisible] = useState(false);
  const [cognitions, setCognitions] = useState<Cognition[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [filterMode, setFilterMode] = useState<'session' | 'all'>('session');
  const [page, setPage] = useState(0);
  const [total, setTotal] = useState(0);
  const sessionPath = useStore(s => s.currentSessionPath);

  useEffect(() => {
    if (visible) load();
  }, [visible, filterMode, page]);

  useEffect(() => {
    const handler = () => { setVisible(true); setPage(0); load(); };
    window.addEventListener('hana-view-memories', handler);
    return () => window.removeEventListener('hana-view-memories', handler);
  }, [filterMode, page]);

  const load = async () => {
    setLoading(true);
    setError('');
    try {
      const params: Record<string, unknown> = {
        subject: 'USER',
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      };
      if (filterMode === 'session' && sessionPath) {
        params.scope = sessionPath;
      }
      const data = await loomRpc('memory.cognitions', params);
      setCognitions((data as any)?.cognitions || []);
      setTotal((data as any)?.total || 0);
    } catch (err: any) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: number, trait: string) => {
    if (!window.confirm(`${t('settings.memory.deleteConfirm')} "${trait}"?`)) return;
    try {
      await loomRpc('memory.cognition_delete', { id });
      load();
    } catch (err: any) {
      setError(err.message || String(err));
    }
  };

  const close = useCallback(() => setVisible(false), []);

  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <Overlay
      open={visible}
      onClose={close}
      backdrop="blur"
      zIndex={100}
      className={styles['memory-viewer']}
      disableContainerAnimation
    >
      <div className={styles['memory-viewer-header']}>
        <h3 className={styles['memory-viewer-title']}>
          {t('settings.memory.cognitions')}
          {total > 0 && <span style={{ fontWeight: 400, fontSize: '0.75em', color: 'var(--color-text-secondary)', marginLeft: 8 }}>({total})</span>}
        </h3>
        <div style={{ display: 'flex', gap: 4, alignItems: 'center' }}>
          <select
            value={filterMode}
            onChange={(e) => { setFilterMode(e.target.value as 'session' | 'all'); setPage(0); }}
            style={{ fontSize: '0.8em', padding: '2px 6px', borderRadius: 4, border: '1px solid var(--color-border)' }}
          >
            <option value="session">{t('settings.memory.filterSession')}</option>
            <option value="all">{t('settings.memory.filterAll')}</option>
          </select>
          <button className={styles['memory-viewer-close']} onClick={close}>✕</button>
        </div>
      </div>
      <div className={styles['memory-viewer-body']}>
        {loading ? (
          <div className="memory-viewer-empty">{t('common.loading')}</div>
        ) : error ? (
          <div className="memory-viewer-empty">{error}</div>
        ) : cognitions.length === 0 ? (
          <div className="memory-viewer-empty">{t('settings.memory.empty')}</div>
        ) : (
          <>
            {cognitions.map((c) => (
              <div key={c.id} className="memory-item" style={{
                padding: 'var(--space-sm) var(--space-md)',
                borderBottom: '1px solid var(--color-border)',
              }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 4 }}>
                  <strong>{c.trait}</strong>
                  <span style={{ fontSize: '0.8em', color: 'var(--color-text-secondary)' }}>
                    {(c.confidence * 100).toFixed(0)}% · {c.evidence_count}e · v{c.version}
                  </span>
                </div>
                <div style={{ fontSize: '0.85em', fontFamily: 'monospace', color: 'var(--color-accent)', marginBottom: 4 }}>
                  {confidenceBar(c.confidence)}
                </div>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-end', gap: 'var(--space-sm)' }}>
                  <div style={{ fontSize: '0.9em', color: 'var(--color-text-secondary)', flex: 1 }}>
                    {c.value}
                  </div>
                  <button
                    onClick={() => handleDelete(c.id, c.trait)}
                    title={t('settings.memory.deleteItem')}
                    style={{
                      background: 'none', border: '1px solid var(--color-border)',
                      cursor: 'pointer', color: 'var(--color-text-secondary)',
                      fontSize: '0.75em', padding: '1px 8px', borderRadius: 4,
                      whiteSpace: 'nowrap', flexShrink: 0,
                    }}
                  >
                    {t('settings.memory.deleteItem')}
                  </button>
                </div>
              </div>
            ))}
            {totalPages > 1 && (
              <div style={{
                display: 'flex', justifyContent: 'center', alignItems: 'center',
                gap: 'var(--space-md)', padding: 'var(--space-md)',
                borderTop: '1px solid var(--color-border)',
              }}>
                <button
                  disabled={page === 0}
                  onClick={() => setPage(p => p - 1)}
                  style={{
                    border: '1px solid var(--color-border)', borderRadius: 4,
                    padding: '4px 14px', background: 'var(--color-bg-secondary)',
                    cursor: page === 0 ? 'not-allowed' : 'pointer',
                    opacity: page === 0 ? 0.3 : 1, fontSize: '0.85em',
                  }}
                >
                  ← {t('settings.memory.prev')}
                </button>
                <span style={{ fontSize: '0.85em', color: 'var(--color-text-secondary)' }}>
                  {page + 1} / {totalPages}
                </span>
                <button
                  disabled={page >= totalPages - 1}
                  onClick={() => setPage(p => p + 1)}
                  style={{
                    border: '1px solid var(--color-border)', borderRadius: 4,
                    padding: '4px 14px', background: 'var(--color-bg-secondary)',
                    cursor: page >= totalPages - 1 ? 'not-allowed' : 'pointer',
                    opacity: page >= totalPages - 1 ? 0.3 : 1, fontSize: '0.85em',
                  }}
                >
                  {t('settings.memory.next')} →
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </Overlay>
  );
}
