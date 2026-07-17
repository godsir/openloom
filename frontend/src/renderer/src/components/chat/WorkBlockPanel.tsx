import { useState, useEffect, useRef } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconChevronRight, IconChevronDown } from '../../utils/icons'
import ShellBlock from './ShellBlock'
import ToolGroupBlock from './ToolGroupBlock'
import SubagentCard from './SubagentCard'
import TeamCard from './TeamCard'
import FileBlock from './FileBlock'
import styles from './WorkBlockPanel.module.css'

/** Block types that are grouped into the work-block panel. */
export const WORK_BLOCK_TYPES = new Set(['shell', 'tool_group', 'subagent', 'team', 'file'])

interface Summary {
  toolCount: number
  subagentCount: number
  teamCount: number
  fileCount: number
  totalElapsed: number
}

function buildSummary(blocks: ContentBlock[]): Summary {
  const s: Summary = { toolCount: 0, subagentCount: 0, teamCount: 0, fileCount: 0, totalElapsed: 0 }
  for (const b of blocks) {
    switch (b.type) {
      case 'shell':
      case 'tool_group':
        s.toolCount++
        break
      case 'subagent':
        s.subagentCount++
        break
      case 'team':
        s.teamCount++
        break
      case 'file':
        s.fileCount++
        break
    }
    if (b.type === 'tool_group' && Array.isArray(b.tools)) {
      for (const t of b.tools as any[]) {
        if (typeof t.elapsed === 'number' && t.elapsed > 0) s.totalElapsed += t.elapsed
      }
    }
    if (b.type === 'shell' && typeof (b as any).elapsed === 'number' && (b as any).elapsed > 0) {
      s.totalElapsed += (b as any).elapsed
    }
  }
  return s
}

function fmtElapsed(ms: number): string {
  if (!ms || ms < 0) return ''
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
  const min = Math.floor(ms / 60_000)
  const sec = Math.round((ms % 60_000) / 1000)
  return `${min}m ${sec}s`
}

function renderBlock(block: ContentBlock, key: string) {
  switch (block.type) {
    case 'shell':
      return <ShellBlock key={key} block={block} />
    case 'tool_group':
      return <ToolGroupBlock key={key} block={block} />
    case 'subagent':
      return <SubagentCard key={key} block={block} />
    case 'team':
      return <TeamCard key={key} block={block} />
    case 'file':
      return <FileBlock key={key} block={block} />
    default:
      return null
  }
}

/**
 * WorkBlockPanel — 外层折叠抽屉，将连续的工作块归入一个可收起的面板。
 *
 * 展开判定优先级：用户手动点击 > 流式状态 > 全局偏好设置
 * - 流式传输中（最新消息）默认展开
 * - 历史消息按设置偏好，用户点击后记录手动选择
 * - 全局设置变化时重置手动选择
 */
export default function WorkBlockPanel({
  blocks,
  defaultExpanded,
}: {
  blocks: ContentBlock[]
  defaultExpanded: boolean
}) {
  const { t } = useLocale()
  // null = no manual override, follow preference / streaming
  const [userOverride, setUserOverride] = useState<boolean | null>(null)
  const [prefExpand, setPrefExpand] = useState(true)
  // Track defaultExpanded to detect streaming→done transitions
  const prevDefault = useRef(defaultExpanded)

  // Load preference on mount; listen for live changes from settings
  useEffect(() => {
    window.loom.getPreference('workBlockExpandDefault', true).then(setPrefExpand)
    const handler = (e: Event) => {
      const d = (e as CustomEvent).detail
      if (d?.key === 'work_block_expand') {
        setPrefExpand(d.val)
        // Reset all manual overrides when global pref changes
        setUserOverride(null)
      }
    }
    window.addEventListener('loom-pref-changed', handler)
    return () => window.removeEventListener('loom-pref-changed', handler)
  }, [])

  // When streaming state changes (starts or stops), reset manual override
  // so the automatic behaviour takes over again.
  useEffect(() => {
    if (defaultExpanded !== prevDefault.current) {
      setUserOverride(null)
      prevDefault.current = defaultExpanded
    }
  }, [defaultExpanded])

  // Compute actual expanded state:
  //   1. user clicked → use manual choice
  //   2. streaming active → always expand
  //   3. otherwise → use global preference
  const expanded = userOverride !== null
    ? userOverride
    : defaultExpanded || prefExpand

  const summary = buildSummary(blocks)

  const parts: string[] = []
  if (summary.toolCount > 0) parts.push(`${summary.toolCount} ${t('chat.tools')}`)
  if (summary.subagentCount > 0) parts.push(`${summary.subagentCount} ${t('chat.subAgents')}`)
  if (summary.teamCount > 0) parts.push(`${summary.teamCount} ${t('chat.teams')}`)
  if (summary.fileCount > 0) parts.push(`${summary.fileCount} ${t('chat.files')}`)

  return (
    <div className={styles.panel}>
      <button
        type="button"
        onClick={() => setUserOverride(!expanded)}
        className={styles.toggle}
      >
        {expanded
          ? <IconChevronDown size={10} className={styles.chevron} />
          : <IconChevronRight size={10} className={styles.chevron} />
        }
        <span className={styles.summary}>{parts.join(' · ')}</span>
        {summary.totalElapsed > 0 && (
          <span className={styles.elapsed}>{fmtElapsed(summary.totalElapsed)}</span>
        )}
      </button>
      {expanded && (
        <div className={styles.body}>
          {blocks.map((block, i) => renderBlock(block, `wp-${i}`))}
        </div>
      )}
    </div>
  )
}
