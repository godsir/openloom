// Mermaid diagram renderer — lazy loads mermaid, deduplicates sources.
let mermaidPromise: Promise<unknown> | null = null
const renderedSources = new Set<string>()

export async function renderMermaidDiagram(
  container: HTMLElement,
  source: string,
): Promise<void> {
  // Deduplicate
  if (renderedSources.has(source)) return
  renderedSources.add(source)

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
