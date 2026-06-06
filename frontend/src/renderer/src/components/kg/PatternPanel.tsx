import { useState } from 'react'
import styles from './PatternPanel.module.css'
import type { SessionPatternReport, TopicPattern, ToolPreference, LearningPath, TimePattern } from '../../types/bindings'

const TOPIC_PAGE_SIZE = 15

interface PatternPanelProps {
  report: SessionPatternReport | null
  onRefresh: () => void
}

function formatDate(iso: string): string {
  if (!iso) return '-'
  return iso.split('T')[0]
}

function formatHourBucket(hour: number): string {
  const h = Math.floor(hour)
  const m = Math.round((hour - h) * 60)
  if (m === 0) return String(h).padStart(2, '0') + ':00'
  return String(h).padStart(2, '0') + ':' + String(m).padStart(2, '0')
}

function confidenceColor(confidence: number): string {
  if (confidence >= 0.8) return 'var(--green, #22c55e)'
  if (confidence >= 0.6) return 'var(--yellow, #eab308)'
  return 'var(--red, #ef4444)'
}

// ── Topics Section ────────────────────────────────────────────────────

function TopicsSection({ topics }: { topics: TopicPattern[] }) {
  const [showAll, setShowAll] = useState(false)

  if (topics.length === 0) {
    return <div className={styles.sectionEmpty}>暂无话题数据</div>
  }

  const maxCount = Math.max(...topics.map(t => t.session_count), 1)
  const displayed = showAll ? topics : topics.slice(0, TOPIC_PAGE_SIZE)

  return (
    <div className={styles.topicList}>
      {displayed.map((t, i) => (
        <div key={i} className={styles.topicItem}>
          <div className={styles.topicHeader}>
            <span className={styles.topicName}>{t.topic}</span>
            <span className={styles.topicBadge}>{t.session_count} 次</span>
          </div>
          <div className={styles.topicBar}>
            <div
              className={styles.topicBarFill}
              style={{ width: `${(t.session_count / maxCount) * 100}%` }}
            />
          </div>
          <div className={styles.topicMeta}>
            <span>{formatDate(t.first_seen)}</span>
            <span className={styles.topicMetaSep}>至</span>
            <span>{formatDate(t.last_seen)}</span>
          </div>
        </div>
      ))}
      {topics.length > TOPIC_PAGE_SIZE && (
        <button className={styles.showMoreBtn} onClick={() => setShowAll(!showAll)}>
          {showAll ? '收起' : `显示全部 (${topics.length} 项)`}
        </button>
      )}
    </div>
  )
}

// ── Tools Section ─────────────────────────────────────────────────────

function ToolsSection({ tools }: { tools: ToolPreference[] }) {
  if (tools.length === 0) {
    return <div className={styles.sectionEmpty}>暂无工具使用数据</div>
  }

  const maxCount = Math.max(...tools.map(t => t.usage_count), 1)

  return (
    <div className={styles.toolList}>
      {tools.map((t, i) => (
        <div key={i} className={styles.toolItem}>
          <div className={styles.toolRow}>
            <span className={styles.toolName}>{t.tool}</span>
            <span className={styles.toolCount}>{t.usage_count}</span>
            <span
              className={styles.toolConf}
              style={{ color: confidenceColor(t.avg_confidence) }}
              title={`平均置信度 ${(t.avg_confidence * 100).toFixed(0)}%`}
            >
              {(t.avg_confidence * 100).toFixed(0)}%
            </span>
          </div>
          <div className={styles.toolBar}>
            <div
              className={styles.toolBarFill}
              style={{ width: `${(t.usage_count / maxCount) * 100}%` }}
            />
            <div
              className={styles.toolBarConf}
              style={{
                left: `${t.avg_confidence * 100}%`,
                background: confidenceColor(t.avg_confidence),
              }}
            />
          </div>
        </div>
      ))}
    </div>
  )
}

// ── Learning Progression Section ──────────────────────────────────────

function LearningSection({ learning }: { learning: LearningPath[] }) {
  if (learning.length === 0) {
    return <div className={styles.sectionEmpty}>暂无学习进度数据</div>
  }

  return (
    <div className={styles.learnList}>
      {learning.map((lp, i) => (
        <div key={i} className={styles.learnItem}>
          <div className={styles.learnHeader}>
            <span className={styles.learnDomain}>{lp.domain}</span>
            <span
              className={styles.learnConf}
              style={{ color: confidenceColor(lp.confidence) }}
            >
              {(lp.confidence * 100).toFixed(0)}%
            </span>
          </div>
          <div className={styles.learnStages}>
            {lp.stages.map((stage, j) => (
              <div key={j} className={styles.learnStage}>
                <div className={styles.learnStageDot} />
                {j < lp.stages.length - 1 && <div className={styles.learnStageLine} />}
                <span className={styles.learnStageLabel}>{stage}</span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

// ── Time Patterns Section (24-hour bar chart) ────────────────────────

function TimeSection({ patterns }: { patterns: TimePattern[] }) {
  if (patterns.length === 0) {
    return <div className={styles.sectionEmpty}>暂无时间模式数据</div>
  }

  // Build 24-hour frequency map
  const hourFreq = new Array(24).fill(0)
  for (const p of patterns) {
    const bucket = Math.floor(p.hour_bucket)
    if (bucket >= 0 && bucket < 24) {
      hourFreq[bucket] += p.frequency
    }
  }

  const maxFreq = Math.max(...hourFreq, 1)

  return (
    <div className={styles.timeChart}>
      <div className={styles.timeBars}>
        {hourFreq.map((freq, hour) => (
          <div key={hour} className={styles.timeBarCol} title={`${String(hour).padStart(2, '0')}:00 — ${freq}`}>
            <div className={styles.timeBarWrap}>
              <div
                className={styles.timeBar}
                style={{
                  height: maxFreq > 0 ? `${(freq / maxFreq) * 100}%` : '0%',
                  opacity: freq > 0 ? 0.7 + (freq / maxFreq) * 0.3 : 0.15,
                }}
              />
            </div>
            <span className={styles.timeLabel}>{String(hour).padStart(2, '0')}</span>
          </div>
        ))}
      </div>
      <div className={styles.timeLegend}>
        <span className={styles.timeLegendLabel}>{Math.floor(maxFreq)}</span>
        <span className={styles.timeLegendLabel}>0</span>
      </div>
    </div>
  )
}

// ── Panel ─────────────────────────────────────────────────────────────

export default function PatternPanel({ report, onRefresh }: PatternPanelProps) {
  const isEmpty = !report || (
    report.topics.length === 0 &&
    report.tools.length === 0 &&
    report.learning.length === 0 &&
    report.time_patterns.length === 0
  )

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <h2 className={styles.title}>会话模式</h2>
        <button className={styles.refreshBtn} onClick={onRefresh} title="刷新模式数据">
          ↻ 刷新
        </button>
      </div>

      {isEmpty && (
        <div className={styles.fullEmpty}>
          <p className={styles.fullEmptyText}>暂无模式数据</p>
          <p className={styles.fullEmptyHint}>
            模式数据会在多轮对话后自动分析。点击刷新手动加载最新报告。
          </p>
        </div>
      )}

      {report && (
        <>
          {report.topics.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>
                话题频次
                <span className={styles.sectionCount}>{report.topics.length}</span>
              </div>
              <TopicsSection topics={report.topics} />
            </div>
          )}

          {report.tools.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>
                工具偏好
                <span className={styles.sectionCount}>{report.tools.length}</span>
              </div>
              <ToolsSection tools={report.tools} />
            </div>
          )}

          {report.learning.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>
                学习进展
                <span className={styles.sectionCount}>{report.learning.length}</span>
              </div>
              <LearningSection learning={report.learning} />
            </div>
          )}

          {report.time_patterns.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>
                活跃时段
              </div>
              <TimeSection patterns={report.time_patterns} />
            </div>
          )}
        </>
      )}
    </div>
  )
}
