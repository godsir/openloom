import { useState, useRef, useEffect, useMemo, useCallback } from 'react'
import { useLocale } from '../../i18n'
import type { ContentBlock } from '../../stores/chat'
import { IconChevronRight, IconChevronDown, IconTerminal, IconSearch, IconGlobe, IconFile, IconFileText, IconEdit, IconTrash, IconFolder, IconCheck, IconXCircle } from '../../utils/icons'
import { FileDiffCard } from './FileDiffCard'
import styles from './ShellBlock.module.css'

const FILE_DIFF_TOOLS = new Set(['file_write', 'file_edit'])

// ── ANSI escape sequence parser ──────────────────────────────────────────
// Converts SGR codes (colors, bold, dim, etc.) to styled <span> elements.

interface AnsiState {
  fg: string | null
  bg: string | null
  bold: boolean
  dim: boolean
  italic: boolean
  underline: boolean
  strikethrough: boolean
}

interface AnsiSegment {
  text: string
  style: React.CSSProperties
}

const ANSI_PALETTE: Record<number, string> = {
  30: '#1e1e1e', 31: '#e06c75', 32: '#98c379', 33: '#e5c07b',
  34: '#61afef', 35: '#c678dd', 36: '#56b6c2', 37: '#abb2bf',
  90: '#5c6370', 91: '#e06c75', 92: '#98c379', 93: '#e5c07b',
  94: '#61afef', 95: '#c678dd', 96: '#56b6c2', 97: '#ffffff',
}

const ANSI_BG_PALETTE: Record<number, string> = {
  40: '#1e1e1e', 41: '#e06c75', 42: '#98c379', 43: '#e5c07b',
  44: '#61afef', 45: '#c678dd', 46: '#56b6c2', 47: '#abb2bf',
  100: '#5c6370', 101: '#e06c75', 102: '#98c379', 103: '#e5c07b',
  104: '#61afef', 105: '#c678dd', 106: '#56b6c2', 107: '#ffffff',
}

function stateStyle(s: AnsiState): React.CSSProperties {
  const style: React.CSSProperties = {}
  if (s.bold) style.fontWeight = 700
  if (s.dim) style.opacity = 0.6
  if (s.italic) style.fontStyle = 'italic'
  // Both underline and strikethrough combine via text-decoration
  if (s.underline && s.strikethrough) {
    style.textDecoration = 'underline line-through'
  } else if (s.underline) {
    style.textDecoration = 'underline'
  } else if (s.strikethrough) {
    style.textDecoration = 'line-through'
  }
  // Use CSS variable for foreground so it adapts to the theme's text color contrast.
  // ANSI 30 (black) fg uses --ansi-black which defaults to the theme's secondary text.
  if (s.fg) style.color = s.fg
  if (s.bg) style.backgroundColor = s.bg
  return style
}

function parseAnsi(text: string): AnsiSegment[] {
  const segments: AnsiSegment[] = []
  const re = /\x1b\[([\d;]*)m/g
  let lastIdx = 0
  let state: AnsiState = { fg: null, bg: null, bold: false, dim: false, italic: false, underline: false, strikethrough: false }
  let match: RegExpExecArray | null

  while ((match = re.exec(text)) !== null) {
    if (match.index > lastIdx) {
      segments.push({ text: text.slice(lastIdx, match.index), style: stateStyle(state) })
    }
    const codes = match[1] ? match[1].split(';').map(Number) : [0]
    for (const code of codes) {
      switch (true) {
        case code === 0:
          state = { fg: null, bg: null, bold: false, dim: false, italic: false, underline: false, strikethrough: false }
          break
        case code === 1: state.bold = true; break
        case code === 2: state.dim = true; break
        case code === 3: state.italic = true; break
        case code === 4: state.underline = true; break
        case code === 9: state.strikethrough = true; break
        case code === 22: state.bold = false; state.dim = false; break
        case code === 23: state.italic = false; break
        case code === 24: state.underline = false; break
        case code === 29: state.strikethrough = false; break
        case code >= 30 && code <= 37: state.fg = ANSI_PALETTE[code]; break
        case code === 39: state.fg = null; break
        case code >= 40 && code <= 47: state.bg = ANSI_BG_PALETTE[code]; break
        case code === 49: state.bg = null; break
        case code >= 90 && code <= 97: state.fg = ANSI_PALETTE[code]; break
        case code >= 100 && code <= 107: state.bg = ANSI_BG_PALETTE[code]; break
      }
    }
    lastIdx = match.index + match[0].length
  }
  if (lastIdx < text.length) {
    segments.push({ text: text.slice(lastIdx), style: stateStyle(state) })
  }
  return segments
}

// ── Render a single line with ANSI coloring ──────────────────────────────

function AnsiLine({ text, className }: { text: string; className?: string }) {
  const parts = useMemo(() => parseAnsi(text), [text])
  return (
    <div className={className}>
      {parts.map((p, i) =>
        Object.keys(p.style).length > 0 ? (
          <span key={i} style={p.style}>{p.text}</span>
        ) : (
          p.text
        )
      )}
    </div>
  )
}

export default function ShellBlock({ block }: { block: ContentBlock }) {
  const { t } = useLocale()
  const [expanded, setExpanded] = useState(true)
  const bodyRef = useRef<HTMLDivElement>(null)
  const userScrolledUp = useRef(false)
  const sealed = block.sealed as boolean
  const toolName = (block.toolName as string) || 'unknown'
  const args = block.args as Record<string, unknown> | undefined
  const result = block.result as string | undefined

  // Load default expand preference
  useEffect(() => {
    window.loom.getPreference('toolExpandDefault', true).then(v => setExpanded(v))
    const handler = (e: Event) => {
      const d = (e as CustomEvent).detail
      if (d?.key === 'tool_expand') setExpanded(d.val)
    }
    window.addEventListener('loom-pref-changed', handler)
    return () => window.removeEventListener('loom-pref-changed', handler)
  }, [])

  const status = block.status as string
  const details = block.details as Record<string, unknown> | undefined

  const { label, detail } = formatTool(toolName, args)

  // 终态标识：shell 类工具的 structured_content 携带 exit_code（非 0 即失败）。
  // 此前成功与失败在折叠行上零区别，用户只能展开读 stderr 才知道结果——
  // 现在折叠行直接给出 红叉(失败) / 绿勾(成功) 终态。
  const exitCode = details && typeof details.exit_code === 'number'
    ? (details.exit_code as number)
    : undefined
  const failed = sealed && exitCode !== undefined && exitCode !== 0

  // Smart auto-scroll: only follow output when user is at the bottom
  const handleScroll = useCallback(() => {
    if (!bodyRef.current) return
    const el = bodyRef.current
    // If within 40px of bottom, treat as "at bottom"
    userScrolledUp.current = el.scrollHeight - el.scrollTop - el.clientHeight > 40
  }, [])

  useEffect(() => {
    if (expanded && bodyRef.current && !userScrolledUp.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight
    }
  }, [result, expanded])

  // Reset scroll state when block seals (new command starts fresh)
  useEffect(() => {
    if (sealed) userScrolledUp.current = false
  }, [sealed])

  // Check if this is a file tool with diff data
  const hasDiffData = FILE_DIFF_TOOLS.has(toolName)
    && details
    && typeof details.oldContent === 'string'
    && typeof details.newContent === 'string'
    && details.oldContent !== details.newContent

  return (
    <div className={styles.block}>
      <button
        onClick={() => setExpanded(!expanded)}
        className={styles.toggle}
      >
        {expanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        <ToolIcon name={toolName} />
        <span className={`${styles.label} ${failed ? styles.labelFailed : ''}`}>{label}</span>
        <span className={styles.detail}>{truncate(detail, 120)}</span>
        {!sealed && <span className={styles.dot} />}
        {failed && <IconXCircle size={11} className={styles.failIcon} />}
        {!failed && sealed && exitCode === 0 && <IconCheck size={11} className={styles.okIcon} />}
      </button>
      {expanded && (
        <div ref={bodyRef} className={styles.body} onScroll={handleScroll}>
          {hasDiffData ? (
            <FileDiffCard
              fileName={(details.fileName as string) || (detail.split(/[\\/]/).pop() ?? detail)}
              filePath={(details.filePath as string) || detail}
              oldContent={details.oldContent as string}
              newContent={details.newContent as string}
            />
          ) : (
            <pre className={styles.output}>
              {status === 'running' && !result && (
                <span className={styles.cursor} />
              )}
              {result ? (
                <AnsiOutput text={result} />
              ) : (
                sealed ? t('chat.noOutput') : ''
              )}
            </pre>
          )}
        </div>
      )}
    </div>
  )
}

/** Split result into lines and render each with ANSI parsing. */
function AnsiOutput({ text }: { text: string }) {
  const lines = useMemo(() => text.split('\n'), [text])
  return (
    <>
      {lines.map((line, i) => {
        const isStderr = line.startsWith('[stderr] ')
        const display = isStderr ? line.slice(9) : line
        return (
          <AnsiLine
            key={i}
            text={display}
            className={isStderr ? styles.stderrLine : undefined}
          />
        )
      })}
    </>
  )
}

function ToolIcon({ name }: { name: string }) {
  switch (name) {
    case 'shell': return <IconTerminal size={12} />
    case 'file_write': return <IconEdit size={12} />
    case 'file_read': return <IconFileText size={12} />
    case 'file_edit': return <IconEdit size={12} />
    case 'file_delete': return <IconTrash size={12} />
    case 'content_search': return <IconSearch size={12} />
    case 'file_list': return <IconFolder size={12} />
    case 'web_search': return <IconGlobe size={12} />
    case 'web_fetch': return <IconGlobe size={12} />
    default: return <IconFile size={12} />
  }
}

function formatTool(name: string, args?: Record<string, unknown>): { label: string; detail: string } {
  switch (name) {
    case 'shell': {
      const cmd = (args?.command as string) || ''
      return { label: '$', detail: cmd }
    }
    case 'file_write': {
      const path = (args?.path as string) || (args?.file_path as string) || ''
      return { label: 'Write', detail: path }
    }
    case 'file_read': {
      const path = (args?.path as string) || (args?.file_path as string) || ''
      return { label: 'Read', detail: path }
    }
    case 'file_edit': {
      const path = (args?.path as string) || (args?.file_path as string) || ''
      return { label: 'Edit', detail: path }
    }
    case 'file_delete': {
      const path = (args?.path as string) || (args?.file_path as string) || ''
      return { label: 'Delete', detail: path }
    }
    case 'content_search': {
      const pattern = (args?.pattern as string) || ''
      const dir = (args?.directory as string) || (args?.path as string) || ''
      return { label: 'Search', detail: dir ? `${pattern} in ${dir}` : pattern }
    }
    case 'file_list': {
      const dir = (args?.path as string) || (args?.directory as string) || '.'
      return { label: 'List', detail: dir }
    }
    case 'web_search': {
      const query = (args?.query as string) || ''
      return { label: 'Web', detail: query }
    }
    case 'web_fetch': {
      const url = (args?.url as string) || ''
      return { label: 'Fetch', detail: url }
    }
    default:
      return { label: name, detail: JSON.stringify(args ?? {}).slice(0, 80) }
  }
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s
  return s.slice(0, max) + '...'
}
