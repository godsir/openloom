import { useState, useEffect, useRef } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconZap, IconCheck, IconLoader, IconXCircle, IconChevronRight, IconChevronDown } from '../../utils/icons'
import styles from './ToolGroupBlock.module.css'

interface ToolCall {
  id: string; name: string; status: 'running' | 'done' | 'error'
  elapsed: number; args: Record<string, unknown>; result?: string
}

function toolStatusIcon(s: string) {
  // 三态区分：完成绿勾 / 运行中旋转 loader / 失败红叉（运行中图标此前是静止的，
  // 失败与成功同色，均修复）
  if (s === 'done') return <IconCheck size={10} className={styles.statusDone} />
  if (s === 'running') return <IconLoader size={10} className={styles.statusRunning} />
  return <IconXCircle size={10} className={styles.statusError} />
}

function fmtElapsed(ms: number): string {
  if (!ms || ms < 0) return ''
  if (ms < 1000) return `${Math.round(ms)}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

const RESULT_COLLAPSE_THRESHOLD = 500

export default function ToolGroupBlock({ block }: { block: ContentBlock }) {
  const { t } = useLocale()
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const [expandedResults, setExpandedResults] = useState<Set<string>>(new Set())
  const [copiedId, setCopiedId] = useState<string | null>(null)
  const tools = (block.tools as ToolCall[]) || []
  const collapsed = block.collapsed as boolean | undefined
  const prevCollapsed = useRef(collapsed)

  // Auto-expand running tools; collapse all when done
  useEffect(() => {
    if (!collapsed && prevCollapsed.current !== false) {
      // Tools just became active — expand the first running one
      const running = tools.find(tc => tc.status === 'running')
      if (running) setExpandedId(running.id)
    }
    if (collapsed) {
      setExpandedId(null)
    }
    prevCollapsed.current = collapsed
  }, [collapsed, tools])

  const copyResult = (tool: ToolCall) => {
    if (!tool.result) return
    navigator.clipboard.writeText(tool.result).then(() => {
      setCopiedId(tool.id)
      setTimeout(() => setCopiedId(prev => (prev === tool.id ? null : prev)), 1500)
    })
  }

  const toggleResult = (id: string) => {
    setExpandedResults(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  return (
    <div className={styles.block}>
      {tools.map((tool) => (
        <div key={tool.id} className={styles.row}>
          <button
            onClick={() => setExpandedId(expandedId === tool.id ? null : tool.id)}
            className={styles.toggle}
          >
            <IconZap size={10} className={styles.iconZap} />
            <span className={styles.toolName}>{tool.name}</span>
            {tool.elapsed > 0 && (
              <span className={styles.elapsed}>{fmtElapsed(tool.elapsed)}</span>
            )}
            <span className={styles.statusIcon}>{toolStatusIcon(tool.status)}</span>
            {expandedId !== tool.id
              ? <IconChevronRight size={9} className={styles.chevron} />
              : <IconChevronDown size={9} className={styles.chevron} />}
          </button>
          {expandedId === tool.id && (
            <div className={styles.body}>
              {Object.keys(tool.args).length > 0 && (
                <pre className={styles.args}>
                  {JSON.stringify(tool.args, null, 2)}
                </pre>
              )}
              {tool.result && (
                <div className={styles.resultWrap}>
                  <div className={styles.resultBar}>
                    <button
                      type="button"
                      className={styles.resultBtn}
                      onClick={() => copyResult(tool)}
                    >
                      {copiedId === tool.id ? t('common.copied', '已复制') : t('common.copy', '复制')}
                    </button>
                    {tool.result.length > RESULT_COLLAPSE_THRESHOLD && (
                      <button
                        type="button"
                        className={styles.resultBtn}
                        onClick={() => toggleResult(tool.id)}
                      >
                        {expandedResults.has(tool.id)
                          ? t('common.collapse', '收起')
                          : t('common.expand', '展开')}
                      </button>
                    )}
                  </div>
                  <pre className={`${styles.result} ${expandedResults.has(tool.id) ? styles.resultExpanded : ''}`}>
                    {tool.result}
                  </pre>
                </div>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  )
}
