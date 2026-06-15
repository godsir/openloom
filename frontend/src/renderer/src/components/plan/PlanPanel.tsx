import React, { useEffect, useRef } from 'react'
import { useStore } from '../../stores'

export const PlanPanel: React.FC = () => {
  const plans = useStore(s => s.plans)
  const activePlanId = useStore(s => s.activePlanId)
  const planContent = useStore(s => s.planContent)
  const planPanelOpen = useStore(s => s.planPanelOpen)
  const setPlanContent = useStore(s => s.setPlanContent)
  const setActivePlan = useStore(s => s.setActivePlan)
  const savePlanContent = useStore(s => s.savePlanContent)
  const togglePlanPanel = useStore(s => s.togglePlanPanel)
  const autosaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Load content when activePlan changes
  useEffect(() => {
    if (activePlanId) {
      setActivePlan(activePlanId)
    }
  }, [activePlanId])

  // Debounced autosave: save plan content 650ms after last change
  useEffect(() => {
    if (autosaveTimerRef.current) {
      clearTimeout(autosaveTimerRef.current)
    }
    if (activePlanId && planContent) {
      autosaveTimerRef.current = setTimeout(() => {
        savePlanContent(activePlanId, planContent)
      }, 650)
    }
    return () => {
      if (autosaveTimerRef.current) {
        clearTimeout(autosaveTimerRef.current)
      }
    }
  }, [planContent, activePlanId, savePlanContent])

  const activePlan = plans.find(p => p.id === activePlanId)

  if (!planPanelOpen) return null

  return (
    <div style={{
      width: 340, borderLeft: '1px solid var(--border)',
      display: 'flex', flexDirection: 'column', flex: 1,
      backgroundColor: 'var(--bg-card)'
    }}>
      <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--border)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <span style={{ fontWeight: 600, fontSize: 14 }}>Plan</span>
        <button onClick={togglePlanPanel} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', fontSize: 18 }}>x</button>
      </div>

      {activePlan ? (
        <>
          <div style={{ padding: '8px 16px', fontSize: 13, color: 'var(--text-muted)', borderBottom: '1px solid var(--border)' }}>
            {activePlan.title}
            <span style={{ float: 'right', padding: '2px 6px', borderRadius: 4, fontSize: 11, backgroundColor: 'var(--accent-soft)', color: 'var(--accent)' }}>
              {activePlan.status}
            </span>
          </div>
          <textarea
            value={planContent}
            onChange={e => setPlanContent(e.target.value)}
            style={{
              flex: 1, padding: 16, border: 'none', resize: 'none',
              fontFamily: 'var(--font-mono)', fontSize: 13,
              backgroundColor: 'transparent', color: 'var(--text)',
              outline: 'none'
            }}
            placeholder="Plan content will appear here..."
          />
        </>
      ) : (
        <div style={{ padding: 24, textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
          {plans.length === 0 ? (
            <p>No plans yet. Type <code style={{ backgroundColor: 'var(--surface-subtle)', padding: '2px 4px', borderRadius: 3 }}>/plan your idea</code> in chat.</p>
          ) : (
            <div>
              <p style={{ marginBottom: 12 }}>Select a plan:</p>
              {plans.map(p => (
                <button key={p.id} onClick={() => setActivePlan(p.id)} style={{
                  display: 'block', width: '100%', padding: '8px 12px', marginBottom: 4,
                  border: '1px solid var(--border)', borderRadius: 6,
                  backgroundColor: 'var(--bg)', cursor: 'pointer',
                  textAlign: 'left', fontSize: 13, color: 'var(--text)'
                }}>
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
