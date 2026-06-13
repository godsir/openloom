// Mermaid diagram renderer — lazy loads mermaid, deduplicates per container.
let mermaidPromise: Promise<unknown> | null = null
// Track the last-rendered source per container element. Keyed by the element
// (WeakMap) so entries are GC'd when the container is unmounted — a global
// Set grew unbounded and also blocked re-render of the same source after a
// remount (new element, same source → was incorrectly skipped).
const lastRenderedByContainer = new WeakMap<HTMLElement, string>()

export async function renderMermaidDiagram(
  container: HTMLElement,
  source: string,
): Promise<void> {
  // Deduplicate per container: skip only if this element already shows this source.
  if (lastRenderedByContainer.get(container) === source) return
  lastRenderedByContainer.set(container, source)

  if (!mermaidPromise) {
    mermaidPromise = import('mermaid').then((m) => {
      m.default.initialize({ startOnLoad: false, theme: 'dark', securityLevel: 'strict' })
      return m.default
    })
  }

  try {
    const mermaid = (await mermaidPromise) as { run: (opts: { nodes: HTMLElement[] }) => Promise<void> }
    const id = `mermaid-${Math.random().toString(36).slice(2, 8)}`
    // Set the untrusted source via textContent, never innerHTML — the browser
    // must not parse `source` as HTML before mermaid sanitizes it.
    container.replaceChildren()
    const diagram = document.createElement('div')
    diagram.className = 'mermaid-diagram'
    diagram.id = id
    diagram.textContent = source
    container.appendChild(diagram)
    await mermaid.run({ nodes: [diagram] })
  } catch {
    // Allow a future attempt to retry rendering this source on this container.
    lastRenderedByContainer.delete(container)
    container.textContent = ''
    const errEl = document.createElement('pre')
    errEl.className = 'text-red-400 text-xs'
    errEl.textContent = 'Mermaid render error'
    container.appendChild(errEl)
  }
}

// For testing
export function __setMermaidLoaderForTests(fn: () => Promise<unknown>): void {
  mermaidPromise = fn()
}
