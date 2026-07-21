// PDF Viewer — renders PDF files using pdfjs-dist in Electron
// Worker loaded via Blob URL because Vite ?url paths don't resolve in Electron
// Uses devicePixelRatio for crisp text, and pdfjs TextLayer for text selection.

import React, { useState, useEffect, useRef, useCallback } from 'react';

// Minimal text layer CSS. Mirrors pdfjs-dist/web/pdf_viewer.css rules for .textLayer
// so that the overlay sits on top of the canvas at the same coordinates as the
// glyphs rendered by pdfjs, allowing native text selection + copy.
const TEXT_LAYER_CSS = `
.textLayer {
  position: absolute;
  text-align: initial;
  inset: 0;
  overflow: clip;
  opacity: 0.25;
  line-height: 1;
  text-size-adjust: none;
  forced-color-adjust: none;
  transform-origin: 0 0;
  z-index: 2;
  caret-color: var(--text, #000);
}
.textLayer span,
.textLayer br {
  color: transparent;
  position: absolute;
  white-space: pre;
  cursor: text;
  transform-origin: 0% 0%;
}
.textLayer .endOfContent {
  display: block;
  position: absolute;
  inset: 100% 0 0;
  z-index: -1;
  cursor: default;
  user-select: none;
}
.textLayer.selecting {
  -webkit-user-select: text;
     -moz-user-select: text;
          user-select: text;
}
`;

interface WritePdfViewerProps {
  filePath: string;
  workspaceRoot: string;
}

type ZoomMode = 'fit-width' | 'custom';

// Standard PDF page widths in points (1pt = 1/72 inch). Used as a fallback
// for fit-to-width when the page viewport isn't available yet.
const A4_WIDTH_PT = 595;

export const WritePdfViewer: React.FC<WritePdfViewerProps> = ({ filePath, workspaceRoot }) => {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [numPages, setNumPages] = useState(0);
  const [currentPage, setCurrentPage] = useState(1);
  const [zoomMode, setZoomMode] = useState<ZoomMode>('fit-width');
  const [customScale, setCustomScale] = useState(1.0);
  // pdfReady forces the render effect to re-run after the document is loaded
  // (otherwise the effect's deps never change on first load)
  const [pdfReady, setPdfReady] = useState(0);
  // Container width drives fit-to-width. We measure the scroll container.
  const [containerWidth, setContainerWidth] = useState(0);
  // Force re-render when the container resizes (fit-to-width depends on it)
  const [containerWidthTick, setContainerWidthTick] = useState(0);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const textLayerRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const pdfDocRef = useRef<any>(null);

  // Inject text layer CSS once on mount
  useEffect(() => {
    if (typeof document === 'undefined') return
    const id = 'write-pdf-viewer-textlayer-css'
    if (document.getElementById(id)) return
    const style = document.createElement('style')
    style.id = id
    style.textContent = TEXT_LAYER_CSS
    document.head.appendChild(style)
  }, [])

  // Track the scroll container's width for fit-to-width
  useEffect(() => {
    const el = scrollContainerRef.current
    if (!el) return
    const ro = new ResizeObserver(entries => {
      for (const entry of entries) {
        const w = entry.contentRect.width
        setContainerWidth(w)
        setContainerWidthTick(t => t + 1)
      }
    })
    ro.observe(el)
    setContainerWidth(el.clientWidth)
    return () => ro.disconnect()
  }, [])

  // 缩放处理函数 —— 必须声明在引用它们的键盘/滚轮 effect 之前：effect 的依赖
  // 数组在渲染期即求值，若声明在后会命中暂时性死区（TDZ），打开 PDF 即抛
  // "Cannot access 'zoomIn' before initialization" 崩溃。
  const zoomOut = useCallback(() => {
    setZoomMode('custom')
    setCustomScale(s => Math.max(0.25, +(s - 0.25).toFixed(2)))
  }, [])
  const zoomIn = useCallback(() => {
    setZoomMode('custom')
    setCustomScale(s => Math.min(4.0, +(s + 0.25).toFixed(2)))
  }, [])
  const fitWidth = useCallback(() => {
    setZoomMode('fit-width')
  }, [])
  const setActual = useCallback(() => {
    setZoomMode('custom')
    setCustomScale(1.0)
  }, [])

  // Keyboard shortcuts (← → Space PageUp/Down Home End) — document-level so they
  // work even when the PDF pane doesn't have focus. Skipped when the user is
  // typing into an input/textarea/contenteditable so we don't hijack the chat box.
  useEffect(() => {
    if (loading || error) return
    const isEditable = (el: EventTarget | null) => {
      if (!(el instanceof HTMLElement)) return false
      const tag = el.tagName
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
      if (el.isContentEditable) return true
      return false
    }
    const onKeyDown = (e: KeyboardEvent) => {
      if (isEditable(e.target)) return
      // Don't interfere with modifier-only chords except the ones we own
      if (e.ctrlKey || e.metaKey || e.altKey) {
        if (e.key === '0' && (e.ctrlKey || e.metaKey)) {
          e.preventDefault()
          setActual()
        }
        return
      }
      switch (e.key) {
        case 'ArrowLeft':
        case 'PageUp':
          if (currentPage > 1) {
            e.preventDefault()
            setCurrentPage(p => Math.max(1, p - 1))
          }
          break
        case 'ArrowRight':
        case 'PageDown':
        case ' ': // Space
          if (currentPage < numPages) {
            e.preventDefault()
            setCurrentPage(p => Math.min(numPages, p + 1))
          }
          break
        case 'Home':
          if (currentPage !== 1) {
            e.preventDefault()
            setCurrentPage(1)
          }
          break
        case 'End':
          if (currentPage !== numPages) {
            e.preventDefault()
            setCurrentPage(numPages)
          }
          break
        case '+':
        case '=':
          e.preventDefault()
          zoomIn()
          break
        case '-':
        case '_':
          e.preventDefault()
          zoomOut()
          break
        case '0':
          e.preventDefault()
          setActual()
          break
        case 'f':
        case 'F':
          e.preventDefault()
          fitWidth()
          break
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [loading, error, currentPage, numPages, zoomIn, zoomOut, setActual, fitWidth])

  // Ctrl + mouse wheel = zoom. Attached to the scroll container so the
  // page still scrolls when ctrl is not held.
  useEffect(() => {
    const el = scrollContainerRef.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return
      e.preventDefault()
      if (e.deltaY < 0) {
        zoomIn()
      } else if (e.deltaY > 0) {
        zoomOut()
      }
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [zoomIn, zoomOut])

  // Load the PDF document
  useEffect(() => {
    let cancelled = false
    let blobUrl: string | null = null

    const loadPdf = async () => {
      try {
        setLoading(true)
        setError(null)
        setPdfReady(0)

        const result = await (window as any).loom.readWorkspaceBinary(filePath, workspaceRoot)
        if (cancelled) return

        if (!result || !result.ok || !result.data) {
          setError(result?.message || 'Failed to read PDF file')
          setLoading(false)
          return
        }

        const [pdfjsLib, workerModule] = await Promise.all([
          import('pdfjs-dist'),
          import('pdfjs-dist/build/pdf.worker.min.mjs?raw'),
        ])

        // Build a Blob URL for the worker (required in Electron because
        // Vite ?url paths don't resolve correctly in the Electron renderer).
        const workerBlob = new Blob([workerModule.default], { type: 'application/javascript' })
        blobUrl = URL.createObjectURL(workerBlob)
        pdfjsLib.GlobalWorkerOptions.workerSrc = blobUrl

        const binary = atob(result.data)
        const pdfData = new Uint8Array(binary.length)
        for (let i = 0; i < binary.length; i++) {
          pdfData[i] = binary.charCodeAt(i)
        }

        const pdf = await pdfjsLib.getDocument({ data: pdfData }).promise
        if (cancelled) return
        pdfDocRef.current = pdf
        setNumPages(pdf.numPages)
        setCurrentPage(1)
        setLoading(false)
        // Bump pdfReady so the render effect re-runs with the loaded document
        setPdfReady(t => t + 1)
      } catch (e: any) {
        if (!cancelled) {
          setError(e.message || 'Failed to load PDF')
          setLoading(false)
        }
      }
    }

    loadPdf()
    return () => {
      cancelled = true
      if (blobUrl) URL.revokeObjectURL(blobUrl)
      pdfDocRef.current = null
    }
  }, [filePath, workspaceRoot])

  // Calculate the effective CSS scale (what the user sees)
  const effectiveScale = (() => {
    if (zoomMode === 'fit-width') {
      if (!containerWidth) return 1.0
      // 16px padding on each side
      const usable = Math.max(0, containerWidth - 32)
      return Math.max(0.25, usable / A4_WIDTH_PT)
    }
    return customScale
  })()

  // Render the current page
  useEffect(() => {
    if (!pdfDocRef.current || !canvasRef.current || !pdfReady) return
    let cancelled = false
    const dpr = window.devicePixelRatio || 1
    const renderScale = effectiveScale * dpr

    const renderPage = async () => {
      try {
        const page = await pdfDocRef.current.getPage(currentPage)
        if (cancelled) return

        const viewport = page.getViewport({ scale: renderScale })
        const canvas = canvasRef.current!
        const outputScale = renderScale / dpr // CSS pixels per pt
        const displayWidth = viewport.width / dpr
        const displayHeight = viewport.height / dpr

        // Clear any previous text layer
        if (textLayerRef.current) {
          textLayerRef.current.innerHTML = ''
          textLayerRef.current.style.width = `${displayWidth}px`
          textLayerRef.current.style.height = `${displayHeight}px`
        }

        canvas.width = viewport.width
        canvas.height = viewport.height
        canvas.style.width = `${displayWidth}px`
        canvas.style.height = `${displayHeight}px`

        const ctx = canvas.getContext('2d')!
        ctx.clearRect(0, 0, canvas.width, canvas.height)
        const renderTask = page.render({ canvasContext: ctx, viewport })
        await renderTask.promise
        if (cancelled) return

        // Render the text layer on top for selection + copy
        if (textLayerRef.current) {
          const pdfjsLib = await import('pdfjs-dist')
          const textContent = await page.streamTextContent()
          if (cancelled) return
          const cssViewport = page.getViewport({ scale: outputScale })
          const textLayer = new pdfjsLib.TextLayer({
            textContentSource: textContent,
            container: textLayerRef.current,
            viewport: cssViewport,
          })
          await textLayer.render()
        }
      } catch (e: any) {
        console.error('[WritePdfViewer] render failed:', e)
        if (!cancelled) {
          setError(e?.message || 'Failed to render page')
        }
      }
    }

    renderPage()
    return () => { cancelled = true }
  }, [currentPage, effectiveScale, pdfReady, containerWidthTick])

  if (loading) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)', fontSize: 13 }}>
        Loading PDF...
      </div>
    )
  }

  if (error) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-error)', gap: 8, fontSize: 13 }}>
        <span>PDF Error</span>
        <span style={{ fontSize: 12, opacity: 0.7 }}>{error}</span>
      </div>
    )
  }

  const zoomLabel = zoomMode === 'fit-width'
    ? '适应宽度'
    : `${Math.round(customScale * 100)}%`

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8, padding: '8px 12px', borderBottom: '1px solid var(--border)', flexShrink: 0, flexWrap: 'wrap' }}>
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
        <button onClick={zoomOut}
          style={{ padding: '2px 8px', border: '1px solid var(--border)', borderRadius: 4, background: 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: 12 }}
          title="缩小">−</button>
        <button onClick={zoomIn}
          style={{ padding: '2px 8px', border: '1px solid var(--border)', borderRadius: 4, background: 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: 12 }}
          title="放大">+</button>
        <span style={{ fontSize: 11, color: 'var(--text-muted)', minWidth: 56, textAlign: 'center' }}>{zoomLabel}</span>
        <button onClick={setActual}
          style={{ padding: '2px 8px', border: '1px solid var(--border)', borderRadius: 4, background: zoomMode === 'custom' && Math.abs(customScale - 1.0) < 0.01 ? 'var(--bg-active)' : 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: 12 }}>
          100%
        </button>
        <button onClick={fitWidth}
          style={{ padding: '2px 8px', border: '1px solid var(--border)', borderRadius: 4, background: zoomMode === 'fit-width' ? 'var(--bg-active)' : 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: 12 }}>
          适应宽度
        </button>
      </div>
      <div ref={scrollContainerRef} style={{ flex: 1, overflow: 'auto', padding: 16, background: 'var(--bg)', textAlign: 'center' }}>
        <div style={{ position: 'relative', display: 'inline-block' }}>
          <canvas ref={canvasRef} style={{ boxShadow: '0 2px 12px rgba(0,0,0,0.2)', display: 'block' }} />
          <div ref={textLayerRef} className="textLayer" />
        </div>
      </div>
    </div>
  )
}
