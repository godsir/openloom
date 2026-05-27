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
      m.default.initialize({ startOnLoad: false, theme: 'dark' })
      return m.default
    })
  }

  try {
    const mermaid = (await mermaidPromise) as { run: (opts: { nodes: HTMLElement[] }) => Promise<void> }
    const id = `mermaid-${Math.random().toString(36).slice(2, 8)}`
    container.innerHTML = `<div class="mermaid-diagram" id="${id}">${source}</div>`
    await mermaid.run({ nodes: [document.getElementById(id)!] })
  } catch {
    container.innerHTML = `<pre class="text-red-400 text-xs">Mermaid render error</pre>`
  }
}

// For testing
export function __setMermaidLoaderForTests(fn: () => Promise<unknown>): void {
  mermaidPromise = fn()
}
