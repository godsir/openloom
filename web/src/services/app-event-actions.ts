import { useStore } from '../stores';
import { activateWorkspaceDesk } from '../stores/desk-actions';
import { applyAgentIdentity, loadAgents } from '../stores/agent-actions';
import { loadSessions, switchSession } from '../stores/session-actions';
import { loadChannels } from '../stores/channel-actions';
import { loadModels } from '../utils/ui-helpers';
import { applyEditorTypography } from '../editor/typography';

interface AppEventActionsConfig {
  requestContextUsage?: (sessionPath: string) => void;
}

let _config: AppEventActionsConfig = {};

export function configureAppEventActions(config: AppEventActionsConfig): void {
  _config = config;
}

export function handleAppEvent(
  typeOrEvent: string | Record<string, unknown>,
  data?: Record<string, unknown>,
  meta?: { source?: string },
): void {
  // Normalize: supports both `handleAppEvent(type, data)` and `handleAppEvent({type, data})`
  let type: string;
  let payload: Record<string, unknown>;
  if (typeof typeOrEvent === 'string') {
    type = typeOrEvent;
    payload = data || {};
  } else {
    type = String(typeOrEvent.type || '');
    payload = (typeOrEvent.data as Record<string, unknown>) || {};
  }

  if (!type) return;

  const s = useStore.getState();

  switch (type) {
    case 'agent-switched': {
      applyAgentIdentity({
        agentName: String(payload.agentName || ''),
        agentId: String(payload.agentId || ''),
      });
      loadSessions().catch(() => {});
      if (payload.sessionPath) {
        switchSession(String(payload.sessionPath)).catch(() => {});
      }
      loadChannels().catch(() => {});
      loadModels().catch(() => {});

      useStore.setState({
        currentAgentId: String(payload.agentId || s.currentAgentId || ''),
      } as any);
      useStore.setState({
        currentChannel: null,
        channelMessages: [],
        channelMembers: [],
        channelTotalUnread: 0,
        channelHeaderName: '',
        channelHeaderMembersText: '',
        channelInfoName: '',
        channelIsDM: false,
        thinkingLevel: 'auto',
        activities: [],
        homeFolder: typeof payload.homeFolder === 'string' ? payload.homeFolder : s.homeFolder,
        workspaceFolders: Array.isArray(payload.workspaceFolders) ? payload.workspaceFolders : [],
        cwdHistory: Array.isArray(payload.cwdHistory) ? payload.cwdHistory : [],
        memoryMasterEnabled: typeof payload.memoryMasterEnabled === 'boolean'
          ? payload.memoryMasterEnabled
          : s.memoryMasterEnabled,
      } as any);
      break;
    }

    case 'models-changed': {
      loadModels().catch(() => {});
      if (_config.requestContextUsage && s.currentSessionPath) {
        _config.requestContextUsage(s.currentSessionPath);
      }
      break;
    }

    case 'agent-updated': {
      const agentId = String(payload.agentId || '');
      if (agentId === s.currentAgentId) {
        applyAgentIdentity({
          agentName: String(payload.agentName || ''),
          agentId,
          yuan: String(payload.yuan || ''),
          ui: { settings: false },
        });
      } else {
        loadAgents().catch(() => {});
      }
      break;
    }

    case 'memory-master-changed': {
      const mmAgentId = String(payload.agentId || '');
      const enabled = !!payload.enabled;
      useStore.setState((prev: any) => {
        const patch: Record<string, unknown> = {};
        if (mmAgentId === prev.currentAgentId) {
          patch.memoryMasterEnabled = enabled;
        }
        patch.agents = (prev.agents || []).map((a: any) =>
          a.id === mmAgentId ? { ...a, memoryMasterEnabled: enabled } : a,
        );
        return patch;
      });
      break;
    }

    case 'theme-changed': {
      const theme = payload.theme;
      if (theme && typeof (window as any).setTheme === 'function') {
        (window as any).setTheme(theme);
      }
      break;
    }

    case 'editor-typography-changed': {
      applyEditorTypography(payload.editor || {});
      break;
    }

    case 'network-proxy-changed': {
      // Forward server events to desktop shell; suppress echo from desktop IPC
      if (meta?.source === 'desktop-ipc') break;
      const platform = (window as any).platform;
      if (platform?.settingsChanged) {
        platform.settingsChanged(type, payload);
      }
      break;
    }

    case 'agent-workspace-changed': {
      const wsAgentId = String(payload.agentId || '');
      const homeFolder = typeof payload.homeFolder === 'string' ? payload.homeFolder : null;

      // Only update state and activate desk for current agent
      if (wsAgentId === s.currentAgentId) {
        useStore.setState({
          homeFolder,
        } as any);
        useStore.setState({
          selectedFolder: homeFolder || (s as any).selectedFolder,
          workspaceFolders: [],
        } as any);
        if (homeFolder) {
          activateWorkspaceDesk(homeFolder).catch(err => {
            console.warn('[app-event] activateWorkspaceDesk failed:', err);
          });
        }
      }
      break;
    }

    default:
      break;
  }
}
