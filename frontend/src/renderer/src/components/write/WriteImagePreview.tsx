import React, { useState, useEffect } from 'react';
import { useWriteStore } from '../../stores/write';

export const WriteImagePreview: React.FC = () => {
  const activeFilePath = useWriteStore((s) => s.activeFilePath);
  const workspaceRoot = useWriteStore((s) => s.workspaceRoot);
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [zoom, setZoom] = useState<'fit' | '100' | '200'>('fit');

  useEffect(() => {
    if (!activeFilePath || !workspaceRoot) return;
    setLoading(true);
    setError(null);

    // Try to read the image as data URL via Electron IPC
    const loadImage = async () => {
      try {
        if ((window as any).loom?.readWorkspaceImage) {
          const result = await (window as any).loom.readWorkspaceImage(activeFilePath, workspaceRoot);
          if (result) {
            setDataUrl(result);
            return;
          }
        }
        setError('Cannot load image');
      } catch (e: any) {
        setError(e.message || 'Failed to load image');
      } finally {
        setLoading(false);
      }
    };

    loadImage();
  }, [activeFilePath, workspaceRoot]);

  if (loading) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)' }}>
        Loading image...
      </div>
    );
  }

  if (error || !dataUrl) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-error)', gap: '8px' }}>
        <span>Image Error</span>
        <span style={{ fontSize: '12px' }}>{error || 'No image data'}</span>
      </div>
    );
  }

  const imgStyle: React.CSSProperties =
    zoom === 'fit'
      ? { maxWidth: '100%', maxHeight: '100%', objectFit: 'contain' }
      : zoom === '100'
        ? { maxWidth: 'none', maxHeight: 'none' }
        : { maxWidth: 'none', maxHeight: 'none', transform: 'scale(2)', transformOrigin: 'top left' };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'rgba(0,0,0,0.5)' }}>
      <div style={{ display: 'flex', gap: '4px', padding: '8px 12px', flexShrink: 0 }}>
        {(['fit', '100', '200'] as const).map((z) => (
          <button
            key={z}
            onClick={() => setZoom(z)}
            style={{
              padding: '2px 8px',
              fontSize: '11px',
              border: '1px solid var(--border)',
              borderRadius: '3px',
              background: zoom === z ? 'var(--bg-active)' : 'transparent',
              color: zoom === z ? 'var(--text-accent)' : 'var(--text-muted)',
              cursor: 'pointer',
            }}
          >
            {z === 'fit' ? '适应' : `${z}%`}
          </button>
        ))}
      </div>
      <div style={{ flex: 1, overflow: 'auto', display: 'flex', justifyContent: 'center', alignItems: zoom === 'fit' ? 'center' : 'flex-start', padding: '16px' }}>
        <img src={dataUrl} alt={activeFilePath?.split('/').pop() || 'image'} style={imgStyle} />
      </div>
    </div>
  );
};
