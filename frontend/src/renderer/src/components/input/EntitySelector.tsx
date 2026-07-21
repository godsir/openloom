import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import { createPortal } from 'react-dom'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import { IconBot, IconUsers, IconChevronDown, IconCheck } from '../../utils/icons'
import { useMenuKeyboard, useClickOutside } from '../shared/menu-hooks'
import styles from './EntitySelector.module.css'

type TabId = 'agent' | 'team'

// 键盘导航用的统一条目模型：default 项 + 各 agent，或各 team
type NavItem =
  | { kind: 'default' }
  | { kind: 'agent'; name: string }
  | { kind: 'team'; id: string }

export default function EntitySelector() {
  const { t } = useLocale()
  const agents = useStore((s) => s.agents)
  const teams = useStore((s) => s.teams)
  const currentSessionId = useStore((s) => s.currentSessionId)
  const sid = currentSessionId || 'default'
  const agentBindingName = useStore((s) => s.sessionAgentBindings[sid])
  const teamBindingId = useStore((s) => s.sessionTeamBindings[sid])

  const [open, setOpen] = useState(false)
  const [tab, setTab] = useState<TabId>(teamBindingId ? 'team' : 'agent')
  const [activeIndex, setActiveIndex] = useState(0)
  const [teamsLoading, setTeamsLoading] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const popoverRef = useRef<HTMLDivElement>(null)
  const itemRefs = useRef<(HTMLDivElement | null)[]>([])

  // Load teams on mount (like agents are loaded by AgentConfigPanel)
  useEffect(() => {
    setTeamsLoading(true)
    loomRpc<{ teams: { id: string; name: string; description: string; strategy: string; captain: unknown; members: unknown[] }[] }>('team.config.list')
      .then((r) => useStore.getState().setTeams((r.teams || []) as any))
      .catch(() => {})
      .finally(() => setTeamsLoading(false))
  }, [])

  // Determine which entity is currently active for the trigger label.
  // Team takes priority: when a team is selected, agent binding is reset to 'default'.
  const activeTeam = teamBindingId
    ? teams.find((tm) => tm.id === teamBindingId)
    : undefined
  const activeAgent = agentBindingName && agentBindingName !== 'default'
    ? agents.find((a) => a.name === agentBindingName)
    : undefined

  const triggerLabel = activeTeam
    ? activeTeam.name
    : activeAgent
      ? activeAgent.name
      : t('input.defaultAgent')

  const hasActiveBinding = !!activeAgent || !!activeTeam

  const validAgents = useMemo(
    () => agents.filter((a) => a.name && a.name !== 'default' && !a.name.startsWith('__team_')),
    [agents],
  )

  // 当前 tab 的可导航条目（供键盘 ↑/↓/Enter 使用）
  const navItems = useMemo<NavItem[]>(() => {
    if (tab === 'agent') {
      return [
        { kind: 'default' },
        ...validAgents.map((a): NavItem => ({ kind: 'agent', name: a.name })),
      ]
    }
    return teams.map((tm): NavItem => ({ kind: 'team', id: tm.id }))
  }, [tab, validAgents, teams])

  // 点击外部关闭（统一 hook）
  useClickOutside(
    (target) =>
      !!triggerRef.current?.contains(target) || !!popoverRef.current?.contains(target),
    () => setOpen(false),
    open,
  )

  const handleSelectAgent = useCallback(
    (name: string) => {
      const sid = currentSessionId || 'default'
      // Mutual exclusion: clear team binding
      useStore.getState().setSessionTeamBinding(sid, '')
      useStore.getState().setSessionAgentBinding(sid, name || 'default')
      loomRpc('session.bind_agent', {
        session_id: sid,
        agent_config_name: name || 'default',
      }).catch(() => {})
      setOpen(false)
    },
    [currentSessionId],
  )

  const handleSelectTeam = useCallback(
    (teamId: string) => {
      const sid = currentSessionId || 'default'
      // Mutual exclusion: clear agent binding
      useStore.getState().setSessionAgentBinding(sid, 'default')
      useStore.getState().setSessionTeamBinding(sid, teamId)
      loomRpc('session.bind_team', {
        session_id: sid,
        team_config_id: teamId,
      }).catch(() => {})
      setOpen(false)
    },
    [currentSessionId],
  )

  const selectItem = useCallback(
    (i: number) => {
      const item = navItems[i]
      if (!item) return
      if (item.kind === 'default') handleSelectAgent('')
      else if (item.kind === 'agent') handleSelectAgent(item.name)
      else handleSelectTeam(item.id)
    },
    [navItems, handleSelectAgent, handleSelectTeam],
  )

  // 键盘导航（仅当前 tab 的列表参与）
  useMenuKeyboard({
    open: open && navItems.length > 0,
    itemCount: navItems.length,
    activeIndex,
    setActiveIndex,
    onSelect: selectItem,
    onClose: () => setOpen(false),
  })

  // 打开或切换 tab 时复位高亮；移动时滚入可视区
  useEffect(() => {
    if (open) setActiveIndex(0)
  }, [open, tab])
  useEffect(() => {
    if (!open) return
    itemRefs.current[activeIndex]?.scrollIntoView({ block: 'nearest' })
  }, [open, activeIndex])

  const handleOpenSettings = useCallback(() => {
    setOpen(false)
    useStore.getState().setAppMode('settings')
  }, [])

  // Position popover relative to trigger
  const getPopoverStyle = useCallback((): React.CSSProperties => {
    if (!triggerRef.current) return {}
    const rect = triggerRef.current.getBoundingClientRect()
    const popoverWidth = 220
    const spaceBelow = window.innerHeight - rect.bottom - 12
    const spaceAbove = rect.top - 12
    const estimatedHeight = 360
    const shouldFlip = spaceBelow < estimatedHeight && spaceAbove > spaceBelow

    const pos: React.CSSProperties = {
      position: 'fixed',
      zIndex: 9999,
      width: popoverWidth,
    }
    if (shouldFlip) {
      pos.bottom = window.innerHeight - rect.top + 4
    } else {
      pos.top = rect.bottom + 4
    }
    // Right-align the popover to the trigger
    pos.right = window.innerWidth - rect.right
    return pos
  }, [])

  const isItemActive = (item: NavItem): boolean => {
    if (item.kind === 'default') return !agentBindingName || agentBindingName === 'default'
    if (item.kind === 'agent') return agentBindingName === item.name
    return teamBindingId === item.id
  }

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setOpen(!open)}
        className={[
          styles.trigger,
          hasActiveBinding ? styles.triggerActive : '',
          open ? styles.triggerOpen : '',
        ].filter(Boolean).join(' ')}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={t('entity.selectorLabel', { name: triggerLabel })}
      >
        <IconBot size={12} />
        <span className={styles.triggerLabel}>{triggerLabel}</span>
        <span className={`${styles.chevron} ${open ? styles.chevronOpen : ''}`}>
          <IconChevronDown size={8} />
        </span>
      </button>

      {open &&
        createPortal(
          <div
            ref={popoverRef}
            style={getPopoverStyle()}
            className={styles.popover}
            role="listbox"
            aria-label={t('entity.selectorLabel', { name: triggerLabel })}
          >
            {/* Tabs */}
            <div className={styles.tabs} role="tablist">
              <button
                role="tab"
                aria-selected={tab === 'agent'}
                className={`${styles.tab} ${tab === 'agent' ? styles.tabActive : ''}`}
                onClick={() => setTab('agent')}
              >
                <IconBot size={13} />
                {t('entity.agentTab')}
              </button>
              <button
                role="tab"
                aria-selected={tab === 'team'}
                className={`${styles.tab} ${tab === 'team' ? styles.tabActive : ''}`}
                onClick={() => setTab('team')}
              >
                <IconUsers size={13} />
                {t('entity.teamTab')}
              </button>
            </div>

            {/* Agent list */}
            {tab === 'agent' && (
              <div className={styles.list}>
                {navItems.map((item, i) => {
                  const label = item.kind === 'default' ? t('input.defaultAgent') : (item as { name: string }).name
                  const selected = isItemActive(item)
                  return (
                    <div
                      key={item.kind === 'default' ? '__default__' : (item as { name: string }).name}
                      ref={(el) => { itemRefs.current[i] = el }}
                      role="option"
                      aria-selected={selected}
                      onMouseEnter={() => setActiveIndex(i)}
                      onClick={() => selectItem(i)}
                      className={[
                        styles.item,
                        selected ? styles.itemActive : '',
                        i === activeIndex && !selected ? styles.itemHighlight : '',
                      ].filter(Boolean).join(' ')}
                    >
                      <span className={styles.itemLabel}>{label}</span>
                      {selected && <IconCheck size={12} className={styles.check} />}
                    </div>
                  )
                })}
                {validAgents.length === 0 && (
                  <div className={styles.empty}>{t('entity.noAgents')}</div>
                )}
              </div>
            )}

            {/* Team list */}
            {tab === 'team' && (
              <div className={styles.list}>
                {teamsLoading ? (
                  <div className={styles.empty}>
                    <span className={styles.spinner} aria-hidden="true" />
                    {t('common.loading')}
                  </div>
                ) : (
                  navItems.map((item, i) => {
                    const tm = teams[i]
                    if (!tm) return null
                    const selected = isItemActive(item)
                    return (
                      <div
                        key={tm.id}
                        ref={(el) => { itemRefs.current[i] = el }}
                        role="option"
                        aria-selected={selected}
                        onMouseEnter={() => setActiveIndex(i)}
                        onClick={() => selectItem(i)}
                        className={[
                          styles.item,
                          selected ? styles.itemActive : '',
                          i === activeIndex && !selected ? styles.itemHighlight : '',
                        ].filter(Boolean).join(' ')}
                      >
                        <span className={styles.itemLabel}>{tm.name}</span>
                        {selected && <IconCheck size={12} className={styles.check} />}
                      </div>
                    )
                  })
                )}
                {!teamsLoading && teams.length === 0 && (
                  <div className={styles.empty}>{t('entity.noTeams')}</div>
                )}
              </div>
            )}

            {/* Footer — manage in settings */}
            <div className={styles.footer}>
              <button
                className={styles.footerLink}
                onClick={handleOpenSettings}
              >
                {t('entity.manageInSettings')}
              </button>
            </div>
          </div>,
          document.body,
        )}
    </>
  )
}
