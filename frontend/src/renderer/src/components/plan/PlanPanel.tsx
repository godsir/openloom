import React, { useEffect, useRef, useCallback } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import styles from './PlanPanel.module.css'

export const PlanPanel: React.FC = () => {
  const { t } = useLocale()
  const plans = useStore(s => s.plans)
  const activePlanId = useStore(s => s.activePlanId)
  const planContent = useStore(s => s.planContent)
  const planContentPlanId = useStore(s => s.planContentPlanId)
  const planPanelOpen = useStore(s => s.planPanelOpen)
  const setPlanContent = useStore(s => s.setPlanContent)
  const setActivePlan = useStore(s => s.setActivePlan)
  const savePlanContent = useStore(s => s.savePlanContent)
  const togglePlanPanel = useStore(s => s.togglePlanPanel)
  const loadPlans = useStore(s => s.loadPlans)
  const autosaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const sessionId = useStore(s => s.currentSessionId)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const defaultWorkspace = useStore(s => s.defaultWorkspace)
  const workspaceRoot = sessionId ? (sessionWorkspaces[sessionId] || defaultWorkspace || '') : (defaultWorkspace || '')

  // 面板打开且工作区已知时加载计划列表（B1：此前 plans 从不加载）
  useEffect(() => {
    if (planPanelOpen && workspaceRoot) {
      loadPlans(workspaceRoot)
    }
  }, [planPanelOpen, workspaceRoot, loadPlans])

  // Load content when activePlan changes
  useEffect(() => {
    if (activePlanId) {
      setActivePlan(activePlanId)
    }
  }, [activePlanId])

  // Debounced autosave: save plan content 650ms after last change.
  // 仅当内容与当前计划绑定时才保存，避免切换计划时把旧内容写进新计划（B5）
  useEffect(() => {
    if (autosaveTimerRef.current) {
      clearTimeout(autosaveTimerRef.current)
    }
    if (activePlanId && planContent && planContentPlanId === activePlanId) {
      autosaveTimerRef.current = setTimeout(() => {
        savePlanContent(activePlanId, planContent)
      }, 650)
    }
    return () => {
      if (autosaveTimerRef.current) {
        clearTimeout(autosaveTimerRef.current)
      }
    }
  }, [planContent, activePlanId, planContentPlanId, savePlanContent])

  const activePlan = plans.find(p => p.id === activePlanId)

  // 状态徽章本地化：缺失键时回退原始枚举值（B14）
  const statusLabel = useCallback((status: string): string => {
    const key = `plan.status.${status}`
    const res = t(key)
    return res === key ? status : res
  }, [t])

  if (!planPanelOpen) return null

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <span className={styles.title}>{t('plan.title')}</span>
        <button onClick={togglePlanPanel} className={styles.closeBtn} aria-label={t('common.close')}>×</button>
      </div>

      {activePlan ? (
        <>
          <div className={styles.planMeta}>
            <span className={styles.planTitle} title={activePlan.title}>{activePlan.title}</span>
            <span className={`${styles.statusBadge} ${activePlan.status === 'error' ? styles.statusError : ''}`}>
              {statusLabel(activePlan.status)}
            </span>
          </div>
          <textarea
            value={planContent}
            onChange={e => setPlanContent(e.target.value)}
            className={styles.editor}
            placeholder={t('plan.placeholder')}
          />
        </>
      ) : (
        <div className={styles.empty}>
          {plans.length === 0 ? (
            <>
              <p>{t('plan.empty')}</p>
              <p className={styles.emptyHint}>{t('plan.emptyHint')}</p>
            </>
          ) : (
            <div>
              <p className={styles.selectTitle}>{t('plan.selectPlan')}</p>
              {plans.map(p => (
                <button key={p.id} onClick={() => setActivePlan(p.id)} className={styles.planItem}>
                  {p.title}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

export default PlanPanel
