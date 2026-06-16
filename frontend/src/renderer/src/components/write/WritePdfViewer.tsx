// PDF Viewer — renders PDF files using pdfjs-dist
// Supports page navigation, zoom, and text selection

import React, { useState, useEffect, useRef } from 'react';

interface WritePdfViewerProps {
  filePath: string;
  workspaceRoot: string;
}

export const WritePdfViewer: React.FC<WritePdfViewerProps> = ({ filePath, workspaceRoot }) => {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [numPages, setNumPages] = useState(0);
  const [currentPage, setCurrentPage] = useState(1);
  const [scale, setScale] = useState(1.0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const pdfDocRef = useRef<any>(null);

  useEffect(() => {
    let cancelled = false;

    const loadPdf = async () => {
      try {
        setLoading(true);
        setError(null);

        // Load binary data via Electron IPC
        const result = await (window as any).loom.readWorkspaceBinary(filePath, workspaceRoot);
        if (cancelled) return;

        if (!result || !result.ok || !result.data) {
          setError(result?.message || 'Failed to read PDF file');
          setLoading(false);
          return;
        }

        const pdfjsLib = await import('pdfjs-dist');
        // Use bundled worker
        pdfjsLib.GlobalWorkerOptions.workerSrc = new URL(
          'pdfjs-dist/build/pdf.worker.min.mjs',
          import.meta.url
        ).toString();

        const binary = atob(result.data);
        const pdfData = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) {
          pdfData[i] = binary.charCodeAt(i);
        }

        const pdf = await pdfjsLib.getDocument({ data: pdfData }).promise;
        if (cancelled) return;
        pdfDocRef.current = pdf;
        setNumPages(pdf.numPages);
        setLoading(false);
      } catch (e: any) {
        if (!cancelled) {
          setError(e.message || 'Failed to load PDF');
          setLoading(false);
        }
      }
    };

    loadPdf();
    return () => { cancelled = true; };
  }, [filePath, workspaceRoot]);

  useEffect(() => {
    if (!pdfDocRef.current || !canvasRef.current) return;
    const renderPage = async () => {
      try {
        const page = await pdfDocRef.current.getPage(currentPage);
        const viewport = page.getViewport({ scale });
        const canvas = canvasRef.current!;
        canvas.width = viewport.width;
        canvas.height = viewport.height;
        const ctx = canvas.getContext('2d')!;
        await page.render({ canvasContext: ctx, viewport }).promise;
      } catch {}
    };
    renderPage();
  }, [currentPage, scale]);

  if (loading) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)' }}>
        Loading PDF...
      </div>
    );
  }

  if (error) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-error)', gap: '8px' }}>
        <span>PDF Error</span>
        <span style={{ fontSize: '12px' }}>{error}</span>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      <div style={{
        display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '12px',
        padding: '8px 12px', borderBottom: '1px solid var(--border)', flexShrink: 0,
      }}>
        <button
          onClick={() => setCurrentPage((p) => Math.max(1, p - 1))}
          disabled={currentPage <= 1}
          style={{ padding: '2px 8px', border: '1px solid var(--border)', borderRadius: '4px', background: 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: '12px' }}
        >← Prev</button>
        <span style={{ fontSize: '12px', color: 'var(--text-muted)' }}>{currentPage} / {numPages}</span>
        <button
          onClick={() => setCurrentPage((p) => Math.min(numPages, p + 1))}
          disabled={currentPage >= numPages}
          style={{ padding: '2px 8px', border: '1px solid var(--border)', borderRadius: '4px', background: 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: '12px' }}
        >Next →</button>
        <span style={{ width: '1px', height: '16px', background: 'var(--border)' }} />
        <button onClick={() => setScale((s) => Math.max(0.5, s - 0.25))} style={{ padding: '2px 6px', border: '1px solid var(--border)', borderRadius: '4px', background: 'transparent', color: 'var(--text)', cursor: 'pointer' }}>−</button>
        <span style={{ fontSize: '11px', color: 'var(--text-muted)', minWidth: '40px', textAlign: 'center' }}>{Math.round(scale * 100)}%</span>
        <button onClick={() => setScale((s) => Math.min(2.0, s + 0.25))} style={{ padding: '2px 6px', border: '1px solid var(--border)', borderRadius: '4px', background: 'transparent', color: 'var(--text)', cursor: 'pointer' }}>+</button>
      </div>
      <div style={{ flex: 1, overflow: 'auto', display: 'flex', justifyContent: 'center', padding: '16px', background: 'var(--bg)' }}>
        <canvas ref={canvasRef} style={{ boxShadow: '0 2px 12px rgba(0,0,0,0.15)' }} />
      </div>
    </div>
  );
};
