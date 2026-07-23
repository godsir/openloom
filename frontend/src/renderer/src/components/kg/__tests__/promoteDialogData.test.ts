import { describe, expect, it } from 'vitest'
import type { KgNode } from '../../../types/bindings'
import { getPromotableEntities } from '../promoteDialogData'

const node = (overrides: Partial<KgNode>): KgNode => ({
  node_id: 1,
  name: '叶晓晓',
  entity_type: 'person',
  description: '',
  confidence: 0.8,
  scope: 'session-1',
  layer: 'semantic',
  ...overrides,
})

describe('getPromotableEntities', () => {
  it('only returns entities from the selected session', () => {
    const sessionNode = node({ node_id: 1 })
    const globalNode = node({ node_id: 2, scope: 'global' })

    expect(getPromotableEntities([sessionNode, globalNode], 'session-1', 0.4))
      .toEqual([sessionNode])
  })

  it('filters selected-session entities below the confidence threshold', () => {
    const highConfidence = node({ node_id: 1, confidence: 0.8 })
    const lowConfidence = node({ node_id: 2, name: '低置信实体', confidence: 0.3 })

    expect(getPromotableEntities([highConfidence, lowConfidence], 'session-1', 0.4))
      .toEqual([highConfidence])
  })
})
