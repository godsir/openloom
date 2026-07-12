import { useState, useRef, useEffect, useCallback } from 'react'
import { createPortal } from 'react-dom'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import { IconBot, IconUsers, IconChevronDown, IconCheck } from '../../utils/icons'
import styles from './EntitySelector.module.css'

type TabId = 'agent' | 'team'

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
  const triggerRef = useRef<HTMLButtonElement>(null)
  const popoverRef = useRef<HTMLDivElement>(null)

  // Load teams on mount (like agents are loaded by AgentConfigPanel)
  useEffect(() => {
    loomRpc<{ teams: { id: string; name: string; description: string; strategy: string; captain: unknown; members: unknown[] }[] }>('team.config.list')
      .then((r) => useStore.getState().setTeams((r.teams || []) as any))
      .catch(() => {})
  }, [])

  // Determine which entity is currently active for the trigger label.
  // Team takes priority: when a team is selected, agent binding is reset to 'default'.
  const activeTeam = teamBindingId
    ? teams.find((t) => t.id === teamBindingId)
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

  // Close popover on outside click
  useEffect(() => {
    if (!open) return
    const handler = (e: MouseEvent) => {
      const target = e.target as Node
      if (triggerRef.current?.contains(target)) return
      if (popoverRef.current?.contains(target)) return
      setOpen(false)
    }
    const timer = setTimeout(
      () => document.addEventListener('mousedown', handler),
      0,
    )
    return () => {
      clearTimeout(timer)
      document.removeEventListener('mousedown', handler)
    }
  }, [open])

  const validAgents = agents.filter((a) => a.name && a.name !== 'default' && !a.name.startsWith('__team_'))

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

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setOpen(!open)}
        className={`${styles.trigger} ${hasActiveBinding ? styles.triggerActive : ''}`}
      >
        <IconBot size={12} />
        <span className={styles.triggerLabel}>{triggerLabel}</span>
        <IconChevronDown size={8} />
      </button>

      {open &&
        createPortal(
          <div
            ref={popoverRef}
            style={getPopoverStyle()}
            className={styles.popover}
          >
            {/* Tabs */}
            <div className={styles.tabs}>
              <button
                className={`${styles.tab} ${tab === 'agent' ? styles.tabActive : ''}`}
                onClick={() => setTab('agent')}
              >
                <IconBot size={13} />
                {t('entity.agentTab')}
              </button>
              <button
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
                <div
                  className={`${styles.item} ${!agentBindingName || agentBindingName === 'default' ? styles.itemActive : ''}`}
                  onClick={() => handleSelectAgent('')}
                >
                  <span className={styles.itemLabel}>{t('input.defaultAgent')}</span>
                  {(!agentBindingName || agentBindingName === 'default') && (
                    <IconCheck size={12} className={styles.check} />
                  )}
                </div>
                {validAgents.map((a) => (
                  <div
                    key={a.name}
                    className={`${styles.item} ${agentBindingName === a.name ? styles.itemActive : ''}`}
                    onClick={() => handleSelectAgent(a.name)}
                  >
                    <span className={styles.itemLabel}>{a.name}</span>
                    {agentBindingName === a.name && (
                      <IconCheck size={12} className={styles.check} />
                    )}
                  </div>
                ))}
                {validAgents.length === 0 && (
                  <div className={styles.empty}>{t('entity.noAgents')}</div>
                )}
              </div>
            )}

            {/* Team list */}
            {tab === 'team' && (
              <div className={styles.list}>
                {teams.map((tm) => (
                  <div
                    key={tm.id}
                    className={`${styles.item} ${teamBindingId === tm.id ? styles.itemActive : ''}`}
                    onClick={() => handleSelectTeam(tm.id)}
                  >
                    <span className={styles.itemLabel}>{tm.name}</span>
                    {teamBindingId === tm.id && (
                      <IconCheck size={12} className={styles.check} />
                    )}
                  </div>
                ))}
                {teams.length === 0 && (
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
