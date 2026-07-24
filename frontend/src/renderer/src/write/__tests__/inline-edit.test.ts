import { describe, expect, it } from 'vitest'
import {
  resolveEditScope,
  buildInlineEditPrompt,
  parseInlineEditResponse,
  applyInlineEditReplacement,
  buildDiffChunks,
} from '../inline-edit'
import type { WriteEditorSelectionState } from '../../stores/write'

function makeSelection(text: string, from: number, to: number): WriteEditorSelectionState {
  return {
    source: 'markdown',
    text,
    from,
    to,
    lineFrom: 0,
    lineTo: 0,
    blockType: null,
    containsImage: false,
  }
}

describe('resolveEditScope', () => {
  const content = '第一段开头。\n\n第二段中间内容，需要润色。\n\n第三段结尾。'

  it('selection 粒度返回选区本身与前后文窗口', () => {
    const from = content.indexOf('第二段')
    const to = content.indexOf('。', from) + 1
    const scope = resolveEditScope(content, makeSelection(content.slice(from, to), from, to))
    expect(scope.text).toBe('第二段中间内容，需要润色。')
    expect(scope.from).toBe(from)
    expect(scope.to).toBe(to)
    expect(scope.prefix).toContain('第一段开头。')
    expect(scope.suffix).toContain('第三段结尾。')
  })

  it('前后文窗口在文档首尾处截断不越界', () => {
    const scope = resolveEditScope(content, makeSelection('第一段开头。', 0, 6))
    expect(scope.prefix).toBe('')
    expect(scope.text).toBe('第一段开头。')
  })
})

describe('buildInlineEditPrompt', () => {
  it('包含 PREFIX/EDIT_SCOPE/SUFFIX 标记与指令', () => {
    const prompt = buildInlineEditPrompt({
      prefix: '前文',
      editScope: '待改写',
      suffix: '后文',
      instruction: '润色',
      filePath: 'a.md',
    })
    expect(prompt).toContain('<<<PREFIX')
    expect(prompt).toContain('<<<EDIT_SCOPE')
    expect(prompt).toContain('待改写')
    expect(prompt).toContain('<<<SUFFIX')
    expect(prompt).toContain('Instruction: 润色')
    expect(prompt).toContain('<<<EDIT')
  })
})

describe('parseInlineEditResponse', () => {
  it('解析标准 EDIT 标记', () => {
    expect(parseInlineEditResponse('<<<EDIT\n改写后的文字\n<<</EDIT')).toBe('改写后的文字')
  })

  it('解析包裹在多余说明中的 EDIT 标记', () => {
    expect(parseInlineEditResponse('好的，以下是修改：\n<<<EDIT\n新文本\n<<</EDIT\n希望满意')).toBe('新文本')
  })

  it('兼容 SHORT/LONG 标记', () => {
    expect(parseInlineEditResponse('<<<SHORT\n短\n<<</SHORT')).toBe('短')
    expect(parseInlineEditResponse('<<<LONG\n长\n<<</LONG')).toBe('长')
  })

  it('无标记的纯文本响应原样返回', () => {
    expect(parseInlineEditResponse('直接就是改写结果')).toBe('直接就是改写结果')
  })

  it('空响应返回 null', () => {
    expect(parseInlineEditResponse('')).toBeNull()
    expect(parseInlineEditResponse('<<<EDIT<<</EDIT 不完整')).toBeNull()
  })
})

describe('applyInlineEditReplacement + buildDiffChunks', () => {
  const content = '前文AAA待替换BBB后文'
  const from = content.indexOf('待替换')
  const to = from + 3

  it('按偏移替换选区内容', () => {
    const scope = resolveEditScope(content, makeSelection('待替换', from, to))
    expect(applyInlineEditReplacement(content, scope, '已替换')).toBe('前文AAA已替换BBB后文')
  })

  it('diff chunk 记录原文/新文与双侧偏移', () => {
    const scope = resolveEditScope(content, makeSelection('待替换', from, to))
    const chunks = buildDiffChunks(content, scope, '已替换')
    expect(chunks).toHaveLength(1)
    expect(chunks[0].originalText).toBe('待替换')
    expect(chunks[0].modifiedText).toBe('已替换')
    expect(chunks[0].fromA).toBe(from)
    expect(chunks[0].toA).toBe(to)
    expect(chunks[0].accepted).toBeNull()
  })
})
