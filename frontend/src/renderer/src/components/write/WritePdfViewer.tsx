// PDF Viewer — renders PDF files using pdfjs-dist
// Worker is loaded via Vite ?url import for correct bundling in Electron

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

        const result = await (window as any).loom.readWorkspaceBinary(filePath, workspaceRoot);
        if (cancelled) return;

        if (!result || !result.ok || !result.data) {
          setError(result?.message || 'Failed to read PDF file');
          setLoading(false);
          return;
        }

        // Lazy-load pdfjs + its worker via Vite ?url import
        const [pdfjsLib, { default: workerUrl }] = await Promise.all([
          import('pdfjs-dist'),
          import('pdfjs-dist/build/pdf.worker.min.mjs?url'),
        ]);
        pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl;

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
        canvas.height = viewport.height;
        canvas.width = viewport.width;
        const ctx = canvas.getContext('2d')!;
        await page.render({ canvasContext: ctx, viewport }).promise;
      } catch {}
    };
    renderPage();
  }, [currentPage, scale]);

  if (loading) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)', fontSize: 13 }}>
        Loading PDF...
      </div>
    );
  }

  if (error) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-error)', gap: 8, fontSize: 13 }}>
        <span>PDF Error</span>
        <span style={{ fontSize: 12, opacity: 0.7 }}>{error}</span>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {/* Toolbar */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 12, padding: '8px 12px', borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
        <button onClick={() => setCurrentPage(p => Math.max(1, p - 1))} disabled={currentPage <= 1}
          style={{ padding: '2px 8px', border: '1px solid var(--border)', borderRadius: 4, background: 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: 12, opacity: currentPage <= 1 ? 0.4 : 1 }}>
          ← Prev
        </button>
        <span style={{ fontSize: 12, color: 'var(--text-muted)', minWidth: 60, textAlign: 'center' }}>{currentPage} / {numPages}</span>
        <button onClick={() => setCurrentPage(p => Math.min(numPages, p + 1))} disabled={currentPage >= numPages}
          style={{ padding: '2px 8px', border: '1px solid var(--border)', borderRadius: 4, background: 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: 12, opacity: currentPage >= numPages ? 0.4 : 1 }}>
          Next →
        </button>
        <span style={{ width: 1, height: 16, background: 'var(--border)' }} />
        <button onClick={() => setScale(s => Math.max(0.5, s - 0.25))}
          style={{ padding: '2px 6px', border: '1px solid var(--border)', borderRadius: 4, background: 'transparent', color: 'var(--text)', cursor: 'pointer' }}>−</button>
        <span style={{ fontSize: 11, color: 'var(--text-muted)', minWidth: 40, textAlign: 'center' }}>{Math.round(scale * 100)}%</span>
        <button onClick={() => setScale(s => Math.min(2.0, s + 0.25))}
          style={{ padding: '2px 6px', border: '1px solid var(--border)', borderRadius: 4, background: 'transparent', color: 'var(--text)', cursor: 'pointer' }}>+</button>
      </div>
      {/* Canvas */}
      <div style={{ flex: 1, overflow: 'auto', display: 'flex', justifyContent: 'center', padding: 16, background: 'var(--bg)' }}>
        <canvas ref={canvasRef} style={{ boxShadow: '0 2px 12px rgba(0,0,0,0.2)' }} />
      </div>
    </div>
  );
};
