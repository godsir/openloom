import { useState, useEffect, useCallback } from 'react';
import { useSettingsStore } from '../store';
import { useStore } from '../../stores';
import { loomRpc } from '../../adapter';
import { t } from '../helpers';
import { Overlay } from '../../ui';
import styles from '../Settings.module.css';

export function ClearMemoryConfirm() {
  const showToast = useSettingsStore(s => s.showToast);
  const [visible, setVisible] = useState(false);
  const sessionPath = useStore(s => s.currentSessionPath);
  const [clearing, setClearing] = useState(false);

  useEffect(() => {
    const handler = () => setVisible(true);
    window.addEventListener('hana-show-clear-confirm', handler);
    return () => window.removeEventListener('hana-show-clear-confirm', handler);
  }, []);

  const close = useCallback(() => setVisible(false), []);

  const doClear = async () => {
    setClearing(true);
    try {
      // Rollback all cognitions for USER subject by iterating through them
      const params: Record<string, unknown> = { subject: 'USER', limit: 200 };
      if (sessionPath) params.scope = sessionPath;
      const data = await loomRpc('memory.cognitions', params);
      const cognitions = (data as any)?.cognitions || [];
      for (const c of cognitions) {
        for (let v = c.version; v >= 1; v--) {
          try {
            await loomRpc('memory.cognition_rollback', { cognition_id: c.id, version: v });
          } catch {
            // Rollback may fail if snapshot doesn't exist; try next version
          }
        }
      }
      showToast(t('settings.memory.cleared'), 'success');
    } catch (err: any) {
      showToast(t('settings.saveFailed') + ': ' + err.message, 'error');
    } finally {
      setClearing(false);
      close();
    }
  };

  return (
    <Overlay open={visible} onClose={close} backdrop="blur" zIndex={100} className={styles['memory-confirm-card']} disableContainerAnimation>
      <p className={styles['memory-confirm-text']}>{t('settings.memory.clearConfirm')}</p>
      <div className={styles['memory-confirm-actions']}>
        <button className={styles['memory-confirm-cancel']} onClick={close} disabled={clearing}>
          {t('settings.memory.cancel')}
        </button>
        <button className={styles['memory-confirm-danger']} onClick={doClear} disabled={clearing}>
          {clearing ? t('common.clearing') : t('settings.memory.confirmClear')}
        </button>
      </div>
    </Overlay>
  );
}
