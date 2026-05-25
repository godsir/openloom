// @ts-nocheck
import { useState, useCallback } from 'react';
import type { ActivePanel } from '../../types';
import { useStore } from '../../stores';
import { switchAgent } from '../../stores/agent-actions';
import { RegionalErrorBoundary } from '../RegionalErrorBoundary';
import { SessionList } from '../SessionList';

interface ChatSidebarProps {
  open: boolean;
  showSettingsButton?: boolean;
  onNewSession: () => void;
  onCollapse: () => void;
  onOpenSettings?: () => void;
  onTogglePanel?: (panel: ActivePanel) => void;
  region?: string;
}

export function ChatSidebar({
  open,
  showSettingsButton = true,
  onNewSession,
  onCollapse,
  onOpenSettings,
  onTogglePanel,
  region = 'sidebar',
}: ChatSidebarProps) {
  const currentAgentId = useStore(s => s.currentAgentId);
  const agents = useStore(s => s.agents);
  const t = window.t ?? ((p: string) => p);
  const [selectMode, setSelectMode] = useState(false);

  const handleToggleSelectMode = () => {
    setSelectMode(prev => !prev);
  };

  const handleAgentChange = useCallback((e: React.ChangeEvent<HTMLSelectElement>) => {
    const id = e.target.value;
    if (id && id !== currentAgentId) {
      switchAgent(id);
    }
  }, [currentAgentId]);

  return (
    <aside className={`sidebar${open ? '' : ' collapsed'}`} id="sidebar">
      <div className="sidebar-inner">
        <div className="sidebar-chat-content">
          <div className="sidebar-header">
            <span className="sidebar-title">{t('sidebar.title')}</span>
            <div className="sidebar-header-actions">
              <button className="sidebar-action-btn" id="newSessionBtn" title={t('sidebar.newChat')} onClick={onNewSession}>
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="12" y1="5" x2="12" y2="19"></line>
                  <line x1="5" y1="12" x2="19" y2="12"></line>
                </svg>
              </button>
              {showSettingsButton && (
                <button className="sidebar-action-btn" id="settingsBtn" title={t('settings.title')} onClick={onOpenSettings}>
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                    <circle cx="12" cy="12" r="3"></circle>
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
                  </svg>
                </button>
              )}
              <button
                className={`sidebar-action-btn${selectMode ? ' sidebar-action-btn-active' : ''}`}
                id="sidebarSelectBtn"
                title={selectMode ? t('session.exitSelect') : t('session.select')}
                onClick={handleToggleSelectMode}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                  <polyline points="9 11 12 14 22 4"></polyline>
                  <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"></path>
                </svg>
              </button>
            </div>
          </div>

          {agents && agents.length > 1 && (
            <div className="agent-switcher" style={{ padding: '4px 12px' }}>
              <select
                value={currentAgentId || ''}
                onChange={handleAgentChange}
                style={{
                  width: '100%',
                  padding: '6px 8px',
                  borderRadius: '6px',
                  border: '1px solid var(--border)',
                  background: 'var(--bg-secondary)',
                  color: 'var(--text-primary)',
                  fontSize: '13px',
                  cursor: 'pointer',
                }}
              >
                {agents.map((agent: any) => (
                  <option key={agent.id} value={agent.id}>
                    {agent.name} {agent.isPrimary ? '★' : ''}
                  </option>
                ))}
              </select>
            </div>
          )}

          <div className="session-list" id="sessionList">
            <RegionalErrorBoundary region={region} resetKeys={[currentAgentId]}>
              <SessionList selectMode={selectMode} onExitSelectMode={() => setSelectMode(false)} />
            </RegionalErrorBoundary>
          </div>
        </div>
      </div>
      <div className="resize-handle resize-handle-right" id="sidebarResizeHandle"></div>
    </aside>
  );
}
