import { useState, useEffect, useCallback, useRef } from 'react';
import { useSettingsStore } from '../store';
import { loomRpc } from '../../adapter';
import { t } from '../helpers';
import { renderMarkdown } from '../../utils/markdown';
import { useMermaidDiagrams } from '../../hooks/use-mermaid-diagrams';
import { Overlay } from '../../ui';
import styles from '../Settings.module.css';

export function CompiledMemoryViewer() {
  const [visible, setVisible] = useState(false);
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  useMermaidDiagrams(contentRef, [content, loading]);

  useEffect(() => {
    const handler = () => { setVisible(true); load(); };
    window.addEventListener('hana-view-compiled-memory', handler);
    return () => window.removeEventListener('hana-view-compiled-memory', handler);
  }, []);

  const load = async () => {
    setLoading(true);
    try {
      const data = await loomRpc('memory.persona');
      const summary = (data as any)?.summary || '';
      setContent(summary || t('settings.memory.personaEmpty'));
    } catch (err: any) {
      setContent(`Error: ${err.message}`);
    } finally {
      setLoading(false);
    }
  };

  const close = useCallback(() => setVisible(false), []);

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
        <h3 className={styles['memory-viewer-title']}>{t('settings.memory.persona')}</h3>
        <button className={styles['memory-viewer-close']} onClick={close}>✕</button>
      </div>
      <div className={`${styles['memory-viewer-body']} ${styles['compiled-memory-body']}`}>
        {loading ? (
          <div className="memory-viewer-empty">{t('common.loading')}</div>
        ) : content.trim() ? (
          <div
            ref={contentRef}
            className={`${styles['compiled-memory-md']} md-content`}
            dangerouslySetInnerHTML={{ __html: renderMarkdown(content) }}
          />
        ) : (
          <div className="memory-viewer-empty">{t('settings.memory.personaEmpty')}</div>
        )}
      </div>
    </Overlay>
  );
}
