import { describe, expect, it, vi } from 'vitest'
import type { KgNode } from '../../types/bindings'
import { loadAllKgNodes } from '../loadAllKgNodes'

const node = (node_id: number): KgNode => ({
  node_id,
  name: `node-${node_id}`,
  entity_type: 'concept',
  description: '',
  confidence: 0.8,
  scope: 'global',
  layer: 'semantic',
})

describe('loadAllKgNodes', () => {
  it('loads every page until the server returns a short page', async () => {
    const fetchPage = vi.fn()
      .mockResolvedValueOnce([node(1), node(2)])
      .mockResolvedValueOnce([node(3), node(4)])
      .mockResolvedValueOnce([node(5)])

    const result = await loadAllKgNodes(fetchPage, 2)

    expect(result.map(item => item.node_id)).toEqual([1, 2, 3, 4, 5])
    expect(fetchPage.mock.calls).toEqual([[2, 0], [2, 2], [2, 4]])
  })

  it('deduplicates nodes when page boundaries overlap', async () => {
    const fetchPage = vi.fn()
      .mockResolvedValueOnce([node(1), node(2)])
      .mockResolvedValueOnce([node(2)])

    const result = await loadAllKgNodes(fetchPage, 2)

    expect(result.map(item => item.node_id)).toEqual([1, 2])
  })

  it('stops when the server keeps returning full pages', async () => {
    const fetchPage = vi.fn().mockResolvedValue([node(1), node(2)])

    await expect(loadAllKgNodes(fetchPage, 2, 2))
      .rejects.toThrow('KG node pagination exceeded 2 pages')
    expect(fetchPage).toHaveBeenCalledTimes(3)
  })

  it('accepts a result that exactly fills the configured page limit', async () => {
    const fetchPage = vi.fn()
      .mockResolvedValueOnce([node(1), node(2)])
      .mockResolvedValueOnce([node(3), node(4)])
      .mockResolvedValueOnce([])

    const result = await loadAllKgNodes(fetchPage, 2, 2)

    expect(result.map(item => item.node_id)).toEqual([1, 2, 3, 4])
  })
})
