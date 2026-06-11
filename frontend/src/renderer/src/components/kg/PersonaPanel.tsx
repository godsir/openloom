import { useState, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import type { RichPersona, TechProficiency, Preference, Goal, Approach, Verbosity, Formality } from '../../types/bindings'
import styles from './PersonaPanel.module.css'

// ── helpers ──

const APPROACH_ICONS: Record<Approach, string> = {
  CodeFirst: '\u{2328}',
  PlanFirst: '\u{2630}',
  Conversational: '\u{27F3}',
}

const VERBOSITY_BARS: Record<Verbosity, number> = {
  Concise: 1,
  Balanced: 2,
  Detailed: 3,
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
  const codes: Record<string, string> = { 'zh-CN': '中', 'zh-TW': '繁', 'en-US': 'EN', 'en-GB': 'EN', 'ja-JP': '日', 'ko-KR': '한' }
  return codes[lang] ?? lang.slice(0, 2).toUpperCase()
}

// ── Sub-components ──

function TechStackSection({ techs, t }: {
  techs: TechProficiency[]
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
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

  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>{t('kg.persona.techStack')}</h3>
      <div className={styles.badgeCloud}>
        {techs.map((tech) => (
          <span
            key={tech.name}
            className={styles.proficiencyBadge}
            style={{
              color: PROFICIENCY_COLORS[tech.level] ?? PROFICIENCY_COLORS.Beginner,
              background: PROFICIENCY_BG[tech.level] ?? PROFICIENCY_BG.Beginner,
              borderColor: PROFICIENCY_COLORS[tech.level] ?? PROFICIENCY_COLORS.Beginner,
            }}
            title={`${tech.name} — ${t(`kg.persona.proficiency.${tech.level}`) ?? tech.level} (${t('kg.confidence')}: ${(tech.confidence * 100).toFixed(0)}%)`}
          >
            {tech.name}
            <span className={styles.badgeLevel}>{t(`kg.persona.proficiency.${tech.level}`) ?? tech.level}</span>
          </span>
        ))}
      </div>
    </div>
  )
}

function PreferencesSection({ prefs, t }: {
  prefs: Preference[]
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>{t('kg.persona.preferences')}</h3>
      {prefs.length === 0 ? (
        <div className={styles.miniEmpty}>{t('kg.persona.noPrefs')}</div>
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

function GoalsSection({ goals, t }: {
  goals: Goal[]
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
  const GOAL_STATUS_COLORS: Record<string, string> = {
    Active: 'var(--badge-green)',
    Achieved: 'var(--badge-gray)',
    Abandoned: 'var(--badge-red)',
  }

  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>{t('kg.persona.goals')}</h3>
      {goals.length === 0 ? (
        <div className={styles.miniEmpty}>{t('kg.persona.noGoals')}</div>
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
                {t(`kg.persona.goalStatus.${g.status}`) ?? g.status}
              </span>
              <span className={styles.goalDesc}>{g.description}</span>
              <span className={styles.goalPriority} title={`${t('kg.persona.goalPriority')}: ${g.priority}/10`}>
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

function WorkingStyleSection({ approach, verbosity, t }: {
  approach: Approach
  verbosity: Verbosity
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
  const totalBars = 3
  const filled = VERBOSITY_BARS[verbosity] ?? 2
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>{t('kg.persona.workingStyle')}</h3>
      <div className={styles.styleRow}>
        <div className={styles.styleItem}>
          <span className={styles.styleIcon}>{APPROACH_ICONS[approach] ?? '❓'}</span>
          <div className={styles.styleInfo}>
            <span className={styles.styleLabel}>{t('kg.persona.collabStyle')}</span>
            <span className={styles.styleValue}>{t(`kg.persona.approach.${approach}`) ?? approach}</span>
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
            <span className={styles.styleLabel}>{t('kg.persona.replyDetail')}</span>
            <span className={styles.styleValue}>{t(`kg.persona.verbosity.${verbosity}`) ?? verbosity}</span>
          </div>
        </div>
      </div>
    </div>
  )
}

function CommunicationSection({ language, formality, t }: {
  language: string
  formality: Formality
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
  const FORMALITY_COLORS: Record<Formality, string> = {
    Casual: 'var(--badge-green)',
    Neutral: 'var(--badge-blue)',
    Formal: 'var(--badge-gold)',
  }

  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>{t('kg.persona.communication')}</h3>
      <div className={styles.commRow}>
        <div className={styles.commItem}>
          <span className={styles.langBadge}>{langFlag(language)}</span>
          <span className={styles.commLabel}>{t('kg.persona.language')}</span>
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
            {t(`kg.persona.formality.${formality}`) ?? formality}
          </span>
          <span className={styles.commLabel}>{t('kg.persona.formality')}</span>
          <span className={styles.commValue}>{formality}</span>
        </div>
      </div>
    </div>
  )
}

function ExpertiseSection({ areas, t }: {
  areas: string[]
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
  if (areas.length === 0) return null

  // Expertise areas are dynamic KG-extracted values, not i18n keys — display as-is
  const sizes = ['s', 'm', 'l', 'xl']
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>{t('kg.persona.expertise')}</h3>
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

function BehaviouralSection({ patterns, t }: {
  patterns: string[]
  t: (key: string, vars?: Record<string, string | number>) => string
}) {
  if (patterns.length === 0) return null
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>{t('kg.persona.behavioralPatterns')}</h3>
      <ul className={styles.patternList}>
        {patterns.map((p) => (
          <li key={p} className={styles.patternItem}>{p}</li>
        ))}
      </ul>
    </div>
  )
}

function Spinner({ t }: { t: (key: string) => string }) {
  return (
    <div className={styles.spinnerWrap}>
      <div className={styles.spinner} />
      <span className={styles.spinnerLabel}>{t('common.loading')}</span>
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
  const { t } = useLocale()

  if (loading) {
    return (
      <div className={styles.panel}>
        <Spinner t={t} />
      </div>
    )
  }

  if (!persona) {
    return (
      <div className={styles.panel}>
        <div className={styles.emptyState}>
          <div className={styles.emptyIcon}>
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" opacity="0.4">
              <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
              <circle cx="12" cy="7" r="4" />
            </svg>
          </div>
          <p className={styles.emptyTitle}>{t('kg.persona.noData')}</p>
          <p className={styles.emptyHint}>{t('kg.persona.noDataHint')}</p>
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
          <div className={styles.emptyIcon}>
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" opacity="0.4">
              <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
              <circle cx="12" cy="7" r="4" />
            </svg>
          </div>
          <p className={styles.emptyTitle}>{t('kg.persona.noData')}</p>
          <p className={styles.emptyHint}>{t('kg.persona.noDataHint2')}</p>
        </div>
      </div>
    )
  }

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <h2 className={styles.title}>{t('kg.persona.title')}</h2>
        <button className={styles.refreshBtn} onClick={onRefresh} title={t('kg.persona.refreshTooltip')}>
          {'↻'} {t('kg.persona.refresh')}
        </button>
      </div>

      <TechStackSection techs={persona.tech_stack} t={t} />
      <PreferencesSection prefs={persona.preferences} t={t} />
      <GoalsSection goals={persona.goals} t={t} />
      <WorkingStyleSection
        approach={persona.working_style.approach}
        verbosity={persona.working_style.verbosity}
        t={t}
      />
      <CommunicationSection
        language={persona.communication.language}
        formality={persona.communication.formality}
        t={t}
      />
      <ExpertiseSection areas={persona.expertise_areas} t={t} />
      <BehaviouralSection patterns={persona.behavioural_patterns} t={t} />

      <div className={styles.footer}>
        <span className={styles.timestamp}>
          {t('kg.persona.lastUpdated', { time: formatTimestamp(persona.last_updated) })}
        </span>
      </div>
    </div>
  )
}

// ── Connected wrapper used by parent tabs ──

export function PersonaPanelConnected() {
  const { t } = useLocale()
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
      setError(e?.message ?? t('kg.persona.loadFailed'))
    } finally {
      setLoading(false)
    }
  }, [kgLoadPersona, t])

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
