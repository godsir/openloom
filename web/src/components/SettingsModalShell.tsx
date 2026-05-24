import { useCallback, useEffect, useRef, useState } from 'react';
import { useStore } from '../stores';
import { closeSettingsModal, setSettingsModalActiveTab } from '../stores/settings-modal-actions';
import { useAnimatePresence } from '../hooks/use-animate-presence';
import { SettingsContent } from '../settings/SettingsContent';
import { useSettingsStore } from '../settings/store';
import styles from './SettingsModalShell.module.css';

declare function t(key: string, vars?: Record<string, string | number>): string;

const CLOSE_ANIMATION_MS = 150;
type VisualState = 'opening' | 'open' | 'closing';

export function SettingsModalShell() {
  const settingsModal = useStore(s => s.settingsModal);
  const { mounted, stage } = useAnimatePresence(settingsModal.open, { duration: CLOSE_ANIMATION_MS });
  const [shown, setShown] = useState(false);
  const returnFocusRef = useRef<HTMLElement | null>(null);

  // When modal opens, sync main store activeTab → settings store
  useEffect(() => {
    if (mounted && settingsModal.activeTab) {
      useSettingsStore.getState().set({ activeTab: settingsModal.activeTab });
    }
  }, [mounted, settingsModal.activeTab]);

  // When settings store activeTab changes, sync back to main store
  useEffect(() => {
    if (!mounted) return;
    const unsubscribe = useSettingsStore.subscribe((state, prev) => {
      if (state.activeTab !== prev.activeTab) {
        setSettingsModalActiveTab(state.activeTab);
      }
    });
    return unsubscribe;
  }, [mounted]);

  useEffect(() => {
    if (!mounted) { setShown(false); return; }
    if (stage === 'exit') { setShown(false); return; }
    const id = requestAnimationFrame(() => setShown(true));
    return () => cancelAnimationFrame(id);
  }, [mounted, stage]);

  const visualState: VisualState =
    stage === 'exit' ? 'closing' : shown ? 'open' : 'opening';

  const requestClose = useCallback(() => closeSettingsModal(), []);

  // Save focus before opening, restore on close
  useEffect(() => {
    if (mounted && returnFocusRef.current === null) {
      returnFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    }
    if (!mounted && returnFocusRef.current) {
      returnFocusRef.current.focus?.();
      returnFocusRef.current = null;
    }
  }, [mounted]);

  // ESC to close
  useEffect(() => {
    if (!mounted || stage === 'exit') return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); requestClose(); }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [mounted, stage, requestClose]);

  if (!mounted) return null;

  return (
    <div
      className={`${styles.overlay} ${styles[visualState]}`}
      data-state={visualState}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) requestClose();
      }}
    >
      <div
        className={`${styles.card} ${styles[visualState]}`}
        data-state={visualState}
        role="dialog"
        aria-modal="true"
        aria-label={t('settings.title')}
      >
        <SettingsContent
          variant="modal"
          onClose={requestClose}
        />
      </div>
    </div>
  );
}
