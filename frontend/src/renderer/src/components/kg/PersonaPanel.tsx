import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import type { RichPersona, TechProficiency, Preference, Goal, Approach, Verbosity, Formality } from '../../types/bindings'
import styles from './PersonaPanel.module.css'

// ── helpers ──

const PROFICIENCY_LABELS: Record<string, string> = {
  Beginner: '初学',
  Intermediate: '熟练',
  Advanced: '精通',
  Expert: '专家',
}

const PROFICIENCY_COLORS: Record<string, string> = {
  Beginner: 'var(--badge-gray)',
  Intermediate: 'var(--badge-blue)',
  Advanced: 'var(--badge-green)',
  Expert: 'var(--badge-gold)',
}

const PROFICIENCY_BG: Record<string, string> = {
  Beginner: 'var(--badge-gray-bg)',
  Intermediate: 'var(--badge-blue-bg)',
  Advanced: 'var(--badge-green-bg)',
  Expert: 'var(--badge-gold-bg)',
}

const APPROACH_ICONS: Record<Approach, string> = {
  CodeFirst: '⌨',
  PlanFirst: '\u{1F4CB}',
  Conversational: '\u{1F4AC}',
}

const APPROACH_LABELS: Record<Approach, string> = {
  CodeFirst: '代码先行',
  PlanFirst: '计划先行',
  Conversational: '对话协作',
}

const VERBOSITY_LABELS: Record<Verbosity, string> = {
  Concise: '简洁',
  Balanced: '均衡',
  Detailed: '详细',
}

const VERBOSITY_BARS: Record<Verbosity, number> = {
  Concise: 1,
  Balanced: 2,
  Detailed: 3,
}

const FORMALITY_LABELS: Record<Formality, string> = {
  Casual: '随意',
  Neutral: '中性',
  Formal: '正式',
}

const FORMALITY_COLORS: Record<Formality, string> = {
  Casual: 'var(--badge-green)',
  Neutral: 'var(--badge-blue)',
  Formal: 'var(--badge-gold)',
}

const GOAL_STATUS_LABELS: Record<string, string> = {
  Active: '进行中',
  Achieved: '已完成',
  Abandoned: '已放弃',
}

const GOAL_STATUS_COLORS: Record<string, string> = {
  Active: 'var(--badge-green)',
  Achieved: 'var(--badge-gray)',
  Abandoned: 'var(--badge-red)',
}

const LANG_FLAGS: Record<string, string> = {
  'zh-CN': '\u{1F1E8}\u{1F1F3}',
  'zh-TW': '\u{1F1F9}\u{1F1FC}',
  'en-US': '\u{1F1FA}\u{1F1F8}',
  'en-GB': '\u{1F1EC}\u{1F1E7}',
  'ja-JP': '\u{1F1EF}\u{1F1F5}',
  'ko-KR': '\u{1F1F0}\u{1F1F7}',
}

function formatTimestamp(iso: string): string {
  if (!iso) return '-'
  try {
    const d = new Date(iso)
    return d.toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
  } catch {
    return iso.split('T')[0]
  }
}

function langFlag(lang: string): string {
  return LANG_FLAGS[lang] ?? '\u{1F310}'
}

// ── Sub-components ──

function TechStackSection({ techs }: { techs: TechProficiency[] }) {
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>技术栈</h3>
      <div className={styles.badgeCloud}>
        {techs.map((t) => (
          <span
            key={t.name}
            className={styles.proficiencyBadge}
            style={{
              color: PROFICIENCY_COLORS[t.level] ?? PROFICIENCY_COLORS.Beginner,
              background: PROFICIENCY_BG[t.level] ?? PROFICIENCY_BG.Beginner,
              borderColor: PROFICIENCY_COLORS[t.level] ?? PROFICIENCY_COLORS.Beginner,
            }}
            title={`${t.name} — ${PROFICIENCY_LABELS[t.level] ?? t.level} (证据: ${t.evidence_count}, 确信: ${(t.confidence * 100).toFixed(0)}%)`}
          >
            {t.name}
            <span className={styles.badgeLevel}>{PROFICIENCY_LABELS[t.level] ?? t.level}</span>
          </span>
        ))}
      </div>
    </div>
  )
}

function PreferencesSection({ prefs }: { prefs: Preference[] }) {
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>偏好设置</h3>
      {prefs.length === 0 ? (
        <div className={styles.miniEmpty}>暂无偏好数据</div>
      ) : (
        <div className={styles.prefList}>
          {prefs.map((p, i) => (
            <div key={`${p.key}-${p.value}-${i}`} className={styles.prefRow}>
              <div className={styles.prefInfo}>
                <span className={styles.prefKey}>{p.key}</span>
                <span className={styles.prefValue}>{p.value}</span>
              </div>
              <div className={styles.strengthWrap}>
                <div className={styles.strengthBar}>
                  <div
                    className={styles.strengthFill}
                    style={{ width: `${(p.strength * 100).toFixed(0)}%` }}
                  />
                </div>
                <span className={styles.strengthLabel}>{Math.round(p.strength * 100)}%</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function GoalsSection({ goals }: { goals: Goal[] }) {
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>目标 & 意图</h3>
      {goals.length === 0 ? (
        <div className={styles.miniEmpty}>暂无目标数据</div>
      ) : (
        <div className={styles.goalList}>
          {goals.map((g, i) => (
            <div key={i} className={styles.goalRow}>
              <span
                className={styles.goalStatus}
                style={{
                  color: GOAL_STATUS_COLORS[g.status] ?? GOAL_STATUS_COLORS.Active,
                  background: `${GOAL_STATUS_COLORS[g.status] ?? GOAL_STATUS_COLORS.Active}18`,
                  borderColor: `${GOAL_STATUS_COLORS[g.status] ?? GOAL_STATUS_COLORS.Active}40`,
                }}
              >
                {GOAL_STATUS_LABELS[g.status] ?? g.status}
              </span>
              <span className={styles.goalDesc}>{g.description}</span>
              <span className={styles.goalPriority} title={`优先级 ${g.priority}/10`}>
                {'★'.repeat(Math.min(g.priority, 5))}
                <span className={styles.goalPriorityNum}>{g.priority}</span>
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function WorkingStyleSection({ approach, verbosity }: { approach: Approach; verbosity: Verbosity }) {
  const totalBars = 3
  const filled = VERBOSITY_BARS[verbosity] ?? 2
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>工作风格</h3>
      <div className={styles.styleRow}>
        <div className={styles.styleItem}>
          <span className={styles.styleIcon}>{APPROACH_ICONS[approach] ?? '❓'}</span>
          <div className={styles.styleInfo}>
            <span className={styles.styleLabel}>协作方式</span>
            <span className={styles.styleValue}>{APPROACH_LABELS[approach] ?? approach}</span>
          </div>
        </div>
        <div className={styles.styleDivider} />
        <div className={styles.styleItem}>
          <div className={styles.verbosityBars}>
            {Array.from({ length: totalBars }, (_, i) => (
              <div
                key={i}
                className={`${styles.verbosityBar} ${i < filled ? styles.verbosityBarActive : ''}`}
              />
            ))}
          </div>
          <div className={styles.styleInfo}>
            <span className={styles.styleLabel}>回复详细度</span>
            <span className={styles.styleValue}>{VERBOSITY_LABELS[verbosity] ?? verbosity}</span>
          </div>
        </div>
      </div>
    </div>
  )
}

function CommunicationSection({ language, formality }: { language: string; formality: Formality }) {
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>沟通风格</h3>
      <div className={styles.commRow}>
        <div className={styles.commItem}>
          <span className={styles.langFlag}>{langFlag(language)}</span>
          <span className={styles.commLabel}>语言</span>
          <span className={styles.commValue}>{language}</span>
        </div>
        <div className={styles.styleDivider} />
        <div className={styles.commItem}>
          <span
            className={styles.formalityBadge}
            style={{
              color: FORMALITY_COLORS[formality] ?? FORMALITY_COLORS.Neutral,
              background: `${FORMALITY_COLORS[formality] ?? FORMALITY_COLORS.Neutral}18`,
              borderColor: `${FORMALITY_COLORS[formality] ?? FORMALITY_COLORS.Neutral}40`,
            }}
          >
            {FORMALITY_LABELS[formality] ?? formality}
          </span>
          <span className={styles.commLabel}>正式度</span>
          <span className={styles.commValue}>{formality}</span>
        </div>
      </div>
    </div>
  )
}

function ExpertiseSection({ areas }: { areas: string[] }) {
  if (areas.length === 0) return null
  // Weight: larger font for items appearing earlier (which have higher count)
  const sizes = ['s', 'm', 'l', 'xl']
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>专业领域</h3>
      <div className={styles.tagCloud}>
        {areas.map((area, i) => {
          const size = sizes[Math.min(i, sizes.length - 1)]
          return (
            <span key={area} className={`${styles.tag} ${styles[`tag${size}`]}`}>
              {area}
            </span>
          )
        })}
      </div>
    </div>
  )
}

function BehaviouralSection({ patterns }: { patterns: string[] }) {
  if (patterns.length === 0) return null
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>行为模式</h3>
      <ul className={styles.patternList}>
        {patterns.map((p) => (
          <li key={p} className={styles.patternItem}>{p}</li>
        ))}
      </ul>
    </div>
  )
}

function Spinner() {
  return (
    <div className={styles.spinnerWrap}>
      <div className={styles.spinner} />
      <span className={styles.spinnerLabel}>加载中...</span>
    </div>
  )
}

// ── Main Panel ──

interface PersonaPanelProps {
  persona: RichPersona | null
  onRefresh: () => void
  loading?: boolean
}

export default function PersonaPanel({ persona, onRefresh, loading }: PersonaPanelProps) {
  if (loading) {
    return (
      <div className={styles.panel}>
        <Spinner />
      </div>
    )
  }

  if (!persona) {
    return (
      <div className={styles.panel}>
        <div className={styles.emptyState}>
          <span className={styles.emptyIcon}>{'\u{1F9D0}'}</span>
          <p className={styles.emptyTitle}>暂无用户画像数据</p>
          <p className={styles.emptyHint}>保持与 AI 对话，系统会自动构建你的个人画像</p>
        </div>
      </div>
    )
  }

  const isEmpty =
    persona.tech_stack.length === 0 &&
    persona.preferences.length === 0 &&
    persona.goals.length === 0 &&
    persona.expertise_areas.length === 0 &&
    persona.behavioural_patterns.length === 0 &&
    !persona.working_style.approach &&
    !persona.working_style.verbosity &&
    !persona.communication.language &&
    persona.communication.formality === 'Neutral'

  if (isEmpty) {
    return (
      <div className={styles.panel}>
        <div className={styles.emptyState}>
          <span className={styles.emptyIcon}>{'\u{1F9D0}'}</span>
          <p className={styles.emptyTitle}>暂无用户画像数据</p>
          <p className={styles.emptyHint}>继续对话，系统将自动构建你的用户画像</p>
        </div>
      </div>
    )
  }

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <h2 className={styles.title}>用户画像</h2>
        <button className={styles.refreshBtn} onClick={onRefresh} title="刷新画像数据">
          {'↻'} 刷新
        </button>
      </div>

      <TechStackSection techs={persona.tech_stack} />
      <PreferencesSection prefs={persona.preferences} />
      <GoalsSection goals={persona.goals} />
      <WorkingStyleSection
        approach={persona.working_style.approach}
        verbosity={persona.working_style.verbosity}
      />
      <CommunicationSection
        language={persona.communication.language}
        formality={persona.communication.formality}
      />
      <ExpertiseSection areas={persona.expertise_areas} />
      <BehaviouralSection patterns={persona.behavioural_patterns} />

      <div className={styles.footer}>
        <span className={styles.timestamp}>
          最后更新: {formatTimestamp(persona.last_updated)}
        </span>
      </div>
    </div>
  )
}

// ── Connected wrapper used by parent tabs ──

export function PersonaPanelConnected() {
  const personaData = useStore(s => s.personaData)
  const kgLoadPersona = useStore(s => s.kgLoadPersona)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleRefresh = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      await kgLoadPersona()
    } catch (e: any) {
      setError(e?.message ?? '加载失败')
    } finally {
      setLoading(false)
    }
  }, [kgLoadPersona])

  // Load on mount
  useEffect(() => {
    if (!personaData) {
      handleRefresh()
    }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const richPersona = personaData?.rich_persona ?? null

  return (
    <>
      {error && <div className={styles.errorBanner}>{error}</div>}
      <PersonaPanel persona={richPersona} onRefresh={handleRefresh} loading={loading} />
    </>
  )
}
