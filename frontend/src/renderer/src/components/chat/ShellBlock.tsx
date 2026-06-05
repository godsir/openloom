import { useState, useRef, useEffect } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconChevronRight, IconChevronDown, IconTerminal, IconSearch, IconGlobe, IconFile, IconFileText, IconEdit, IconTrash, IconFolder } from '../../utils/icons'
import { FileDiffCard } from './FileDiffCard'
import styles from './ShellBlock.module.css'

const FILE_DIFF_TOOLS = new Set(['file_write', 'file_edit'])

export default function ShellBlock({ block }: { block: ContentBlock }) {
  const [expanded, setExpanded] = useState(true)
  const bodyRef = useRef<HTMLDivElement>(null)
  const sealed = block.sealed as boolean
  const toolName = (block.toolName as string) || 'unknown'
  const args = block.args as Record<string, unknown> | undefined
  const result = block.result as string | undefined

  // Load default expand preference
  useEffect(() => {
    window.loom.getPreference('toolExpandDefault', true).then(v => setExpanded(v))
  }, [])
  const status = block.status as string
  const details = block.details as Record<string, unknown> | undefined

  const { label, detail } = formatTool(toolName, args)

  useEffect(() => {
    if (expanded && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight
    }
  }, [result, expanded])

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
        <span className={styles.label}>{label}</span>
        <span className={styles.detail}>{truncate(detail, 120)}</span>
        {!sealed && <span className={styles.dot} />}
      </button>
      {expanded && (
        <div ref={bodyRef} className={styles.body}>
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
              {result || (sealed ? '(no output)' : '')}
            </pre>
          )}
        </div>
      )}
    </div>
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
