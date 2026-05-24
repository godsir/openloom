import { useState, useEffect, useCallback } from 'react';
import { loomRpc } from '../../adapter';
import { useStore } from '../../stores';
import { t } from '../helpers';
import { Overlay } from '../../ui';
import styles from '../Settings.module.css';

export function RecordMemoryOverlay() {
  const [visible, setVisible] = useState(false);
  const [text, setText] = useState('');
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<{ count: number } | null>(null);
  const sessionPath = useStore(s => s.currentSessionPath);

  useEffect(() => {
    const handler = () => {
      setVisible(true);
      setText('');
      setResult(null);
    };
    window.addEventListener('hana-record-memory', handler);
    return () => window.removeEventListener('hana-record-memory', handler);
  }, []);

  const close = useCallback(() => setVisible(false), []);

  const saveManual = async () => {
    const trimmed = text.trim();
    if (!trimmed) return;
    setSaving(true);
    try {
      const data = await loomRpc('memory.record', {
        text: trimmed,
        session_id: sessionPath || '_manual',
      });
      setResult({ count: (data as any)?.cognitions ?? 0 });
      setText('');
    } catch (err: any) {
      console.error('[record-memory] failed:', err);
    } finally {
      setSaving(false);
    }
  };

  const saveFromSession = async () => {
    setSaving(true);
    try {
      const data = await loomRpc('memory.record_from_session', {
        session_id: sessionPath || 'default',
      });
      setResult({ count: (data as any)?.cognitions ?? 0 });
    } catch (err: any) {
      console.error('[record-session] failed:', err);
      setResult({ count: -1 });
    } finally {
      setSaving(false);
    }
  };

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
        <h3 className={styles['memory-viewer-title']}>{t('settings.memory.recordTitle')}</h3>
        <button className={styles['memory-viewer-close']} onClick={close}>✕</button>
      </div>
      <div className={styles['memory-viewer-body']} style={{ padding: 'var(--space-md)' }}>

        {result !== null ? (
          <div style={{ textAlign: 'center', padding: 'var(--space-lg)' }}>
            {result.count > 0 ? (
              <p>{t('settings.memory.recordedCount', { count: result.count })}</p>
            ) : result.count === 0 ? (
              <div>
                <p style={{ color: 'var(--color-warning)' }}>{t('settings.memory.recordedZero')}</p>
                <p style={{ fontSize: '0.85em', color: 'var(--color-text-secondary)', marginTop: 4 }}>{t('settings.memory.recordedZeroHint')}</p>
              </div>
            ) : (
              <p style={{ color: 'var(--color-error)' }}>{t('settings.memory.recordedFailed')}</p>
            )}
            <button
              className={styles['settings-save-btn-sm']}
              onClick={() => setResult(null)}
              style={{ marginTop: 'var(--space-md)' }}
            >
              {t('settings.memory.recordAnother')}
            </button>
          </div>
        ) : saving ? (
          <div style={{ textAlign: 'center', padding: 'var(--space-lg)' }}>
            <p>{t('settings.memory.extracting')}</p>
          </div>
        ) : (
          <>
            {/* Option 1: Auto-extract from current session */}
            <div style={{
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: 'var(--space-md)',
              marginBottom: 'var(--space-md)',
              cursor: sessionPath ? 'pointer' : 'not-allowed',
              opacity: sessionPath ? 1 : 0.5,
            }}>
              <div style={{ fontWeight: 600, marginBottom: 4 }}>
                {t('settings.memory.fromSession')}
              </div>
              <div style={{ color: 'var(--color-text-secondary)', fontSize: '0.85em', marginBottom: 'var(--space-sm)' }}>
                {t('settings.memory.fromSessionHint')}
              </div>
              {sessionPath ? (
                <button
                  className={styles['settings-save-btn-sm']}
                  onClick={saveFromSession}
                >
                  {t('settings.memory.extractNow')}
                </button>
              ) : (
                <span style={{ color: 'var(--color-text-secondary)', fontSize: '0.85em' }}>
                  {t('settings.memory.noSession')}
                </span>
              )}
            </div>

            {/* Divider */}
            <div style={{
              display: 'flex', alignItems: 'center', gap: 'var(--space-sm)',
              marginBottom: 'var(--space-md)', color: 'var(--color-text-secondary)',
              fontSize: '0.85em',
            }}>
              <div style={{ flex: 1, height: 1, background: 'var(--color-border)' }} />
              <span>{t('settings.memory.orManual')}</span>
              <div style={{ flex: 1, height: 1, background: 'var(--color-border)' }} />
            </div>

            {/* Option 2: Manual input */}
            <p style={{ color: 'var(--color-text-secondary)', fontSize: '0.85em', marginBottom: 'var(--space-sm)' }}>
              {t('settings.memory.recordHint')}
            </p>
            <textarea
              className={styles['settings-textarea']}
              rows={4}
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder={t('settings.memory.recordPlaceholder')}
              style={{ width: '100%', marginBottom: 'var(--space-md)' }}
              spellCheck={false}
            />
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 'var(--space-sm)' }}>
              <button className={styles['memory-confirm-cancel']} onClick={close}>
                {t('settings.memory.cancel')}
              </button>
              <button
                className={styles['settings-save-btn-sm']}
                onClick={saveManual}
                disabled={!text.trim()}
              >
                {t('settings.memory.record')}
              </button>
            </div>
          </>
        )}
      </div>
    </Overlay>
  );
}
