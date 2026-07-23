import type { KgNode } from '../../types/bindings'

export function getPromotableEntities(
  nodes: KgNode[],
  sessionId: string,
  threshold: number,
): KgNode[] {
  return nodes.filter(node =>
    node.scope === sessionId && node.confidence >= threshold,
  )
}
