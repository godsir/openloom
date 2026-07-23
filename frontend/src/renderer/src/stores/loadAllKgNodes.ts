import type { KgNode } from '../types/bindings'

export async function loadAllKgNodes(
  fetchPage: (limit: number, offset: number) => Promise<KgNode[]>,
  pageSize = 200,
  maxPages = 50,
): Promise<KgNode[]> {
  const nodes = new Map<number, KgNode>()
  for (let pageIndex = 0; pageIndex <= maxPages; pageIndex++) {
    const offset = pageIndex * pageSize
    const page = await fetchPage(pageSize, offset)
    if (pageIndex === maxPages) {
      if (page.length === 0) return [...nodes.values()]
      break
    }
    for (const node of page) nodes.set(node.node_id, node)
    if (page.length < pageSize) return [...nodes.values()]
  }
  throw new Error(`KG node pagination exceeded ${maxPages} pages`)
}
