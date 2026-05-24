// @ts-nocheck
/**
 * session-actions.ts — Session 生命周期操作（纯逻辑 + API）
 *
 * 从 sidebar-shim.ts 迁移。所有函数直接操作 Zustand store，
 * 不依赖 ctx 注入，不持有闭包状态（除 _switchVersion 防竞争）。
 */

/* eslint-disable @typescript-eslint/no-explicit-any -- store partial patch + API 响应 JSON */

import { useStore } from './index';
import { loomRpc, getEnginePort } from '../adapter';
import { buildItemsFromHistory } from '../utils/history-builder';
import { migrateLegacyTodos } from '../utils/todo-compat';
import { loadAvatars as loadAvatarsAction, clearChat as clearChatAction } from './agent-actions';
import { activateWorkspaceDesk } from './desk-actions';
import { loadModels } from '../utils/ui-helpers';
import { updateKeyed } from './create-keyed-slice';
import { snapshotStreamBuffer, type StreamBufferSnapshot } from './stream-invalidator';
import { renderMarkdown } from '../utils/markdown';
import type { ChatMessage, ContentBlock } from './chat-types';
import { readMessageLiveVersion } from './message-live-version';

// ── 防竞争计数器 ──

let _switchVersion = 0;

function invalidateSessionSwitches(): void {
  _switchVersion += 1;
  useStore.setState({ pendingSessionSwitchPath: null });
}

function isCurrentSwitch(version: number, path: string): boolean {
  const state = useStore.getState();
  return version === _switchVersion && state.pendingSessionSwitchPath === path;
}

function isDesktopShell(): boolean {
  return typeof window !== 'undefined' && !!(window as unknown as { hana?: unknown }).hana;
}

function shouldRestoreInputFocus(path: string | null): boolean {
  const state = useStore.getState() as Record<string, any>;
  if (!isDesktopShell()) return false;
  if (state.currentTab !== 'chat') return false;
  if (path) {
    if (state.currentSessionPath !== path) return false;
  } else if (state.pendingNewSession !== true || state.currentSessionPath !== null || state.pendingSessionSwitchPath) {
    return false;
  }
  if (state.settingsModal?.open || state.mediaViewer || state.skillViewerData || state.channelCreateOverlayVisible) return false;
  if (path && state.computerOverlayBySession?.[path]) return false;
  return true;
}

function requestChatInputFocus(path: string | null): void {
  if (shouldRestoreInputFocus(path)) useStore.getState().requestInputFocus?.();
}

async function resetDeskForSessionCwd(cwd?: string | null): Promise<void> {
  // Session 切换后的 cwd 以服务端显式返回值为准；右侧 desk 视图归 workspace/CWD 所有。
  // 切到同一 workspace 时保留当前子目录；切到不同 workspace 时恢复该 workspace 的上次子目录。
  await activateWorkspaceDesk(cwd || null);
}

function clearSessionRuntimeCaches(path: string): void {
  useStore.getState().clearSession?.(path);
  useStore.setState((s: Record<string, any>) => {
    const { [path]: _attached, ...attachedFilesBySession } = s.attachedFilesBySession || {};
    const { [path]: _registryFiles, ...sessionRegistryFilesByPath } = s.sessionRegistryFilesByPath || {};
    const { [path]: _draft, ...drafts } = s.drafts || {};
    const { [path]: _streamMeta, ...sessionStreams } = s.sessionStreams || {};
    const { [path]: _browser, ...browserBySession } = s.browserBySession || {};
    const { [path]: _computerOverlay, ...computerOverlayBySession } = s.computerOverlayBySession || {};
    const { [path]: _scroll, ...scrollPositions } = s.scrollPositions || {};
    const { [path]: _todos, ...todosBySession } = s.todosBySession || {};
    const { [path]: _todosLive, ...todosLiveVersionBySession } = s.todosLiveVersionBySession || {};
    return {
      attachedFilesBySession,
      sessionRegistryFilesByPath,
      drafts,
      sessionStreams,
      browserBySession,
      computerOverlayBySession,
      scrollPositions,
      streamingSessions: (s.streamingSessions || []).filter((sessionPath: string) => sessionPath !== path),
      todosBySession,
      todosLiveVersionBySession,
      inlineErrors: s.inlineErrors ? { ...s.inlineErrors, [path]: null } : s.inlineErrors,
    };
  });
}

// ══════════════════════════════════════════════════════
// 消息加载（从 app-messages-shim 迁移）
// ══════════════════════════════════════════════════════

export async function loadMessages(forPath?: string): Promise<void> {
  const targetPath = forPath || useStore.getState().currentSessionPath;
  if (!targetPath) return;
  const messageLiveVersionBefore = readMessageLiveVersion(targetPath);
  // 捕获 hydrate 前的 live 版本：若 fetch 期间有 tool_end 更新 todos，
  // 后面就跳过 hydrate 写入，避免旧快照覆盖刚收到的实时状态。
  const todosLiveVersionBefore =
    useStore.getState().todosLiveVersionBySession[targetPath] ?? 0;
  // messages 维度的竞态护栏：rapid switch 或并发 load 时，只有最新一次调用
  // 的响应允许 apply initSession，stale 响应直接丢弃。
  const myVersion = useStore.getState().bumpLoadMessagesVersion(targetPath);
  try {
    const data = await loomRpc('session.messages', { path: targetPath });
    const latestVersion =
      useStore.getState()._loadMessagesVersion[targetPath] ?? 0;
    if (latestVersion !== myVersion) {
      // 已经有更新的 loadMessages 在途，stale 响应不应覆盖新状态。
      // todos 与 messages 必须作为同一份 hydrate 快照一起生效或一起丢弃。
      return;
    }
    const messageLiveVersionNow = readMessageLiveVersion(targetPath);
    if (messageLiveVersionNow !== messageLiveVersionBefore) {
      console.log(
        '[loadMessages] 跳过 session hydrate: mid-flight 收到 live message 更新',
        targetPath,
      );
      return;
    }
    const todosLiveVersionNow =
      useStore.getState().todosLiveVersionBySession[targetPath] ?? 0;
    if (todosLiveVersionNow !== todosLiveVersionBefore) {
      console.log(
        '[loadMessages] 跳过 session hydrate: mid-flight 收到 live todo 更新',
        targetPath,
      );
      return;
    }
    // per-session todos（防御性兼容层：即使后端漏转或缓存残留，这里兜底再转一次）
    const rawTodos = data.todos || [];
    const migratedTodos = migrateLegacyTodos({ todos: rawTodos });
    const items = buildItemsFromHistory(data);
    useStore.getState().setSessionRegistryFiles(
      targetPath,
      Array.isArray(data.sessionFiles) ? data.sessionFiles : [],
    );
    useStore.getState().setSessionTodosForPath(targetPath, migratedTodos);
    if (items.length > 0) {
      useStore.getState().initSession(targetPath, items, data.hasMore ?? false);
    } else {
      useStore.getState().initSession(targetPath, [], false);
    }
    // 切换到此 session 后关掉欢迎屏（不管有没有历史消息）
    if (targetPath === useStore.getState().currentSessionPath) {
      useStore.setState({ welcomeVisible: false });
    }
    // In-flight guard: jsonl 仅在 turn_end 落盘。若 session 在 stream 进行中
    // 被 reload（switchSession 冷启动 / stream-resume truncated），合并 buffer
    // 当前快照作为末尾 assistant，避免 UI 上"正在写的消息消失"。
    // 同步执行，不 await，保证中途不会有 text_delta 事件插入。
    const snapshot = snapshotStreamBuffer(targetPath);
    if (snapshot?.hasContent) {
      useStore.getState().appendItem(targetPath, {
        type: 'message',
        data: buildInflightAssistantMessage(snapshot),
      });
    }
  } catch (err) { console.error('[loadMessages] error:', err); }
}

function buildInflightAssistantMessage(snap: StreamBufferSnapshot): ChatMessage {
  const blocks: ContentBlock[] = [];
  if (snap.thinking || snap.inThinking) {
    blocks.push({ type: 'thinking', content: snap.thinking, sealed: !snap.inThinking });
  }
  if (snap.mood) {
    blocks.push({ type: 'mood', yuan: snap.moodYuan, text: snap.mood });
  }
  if (snap.text) {
    const displayText = snap.text.replace(/<tool_code>[\s\S]*?<\/tool_code>\s*/g, '');
    blocks.push({ type: 'text', html: renderMarkdown(displayText), source: displayText });
  }
  return { id: snap.messageId || `inflight-${Date.now()}`, role: 'assistant', blocks, timestamp: Date.now() };
}

/** 上滑加载更早的消息（分页） */
export async function loadMoreMessages(forPath?: string): Promise<void> {
  const targetPath = forPath || useStore.getState().currentSessionPath;
  if (!targetPath) return;
  const session = useStore.getState().chatSessions[targetPath];
  if (!session || !session.hasMore || session.loadingMore) return;

  useStore.getState().setLoadingMore(targetPath, true);
  try {
    const before = session.oldestId ?? '';
    const data = await loomRpc('session.messages', { path: targetPath, before });
    if (Array.isArray(data.sessionFiles)) {
      useStore.getState().setSessionRegistryFiles(targetPath, data.sessionFiles);
    }
    const items = buildItemsFromHistory(data);
    if (items.length > 0) {
      useStore.getState().prependItems(targetPath, items, data.hasMore ?? false);
    } else {
      useStore.getState().setLoadingMore(targetPath, false);
    }
  } catch (err) {
    console.error('[loadMoreMessages] error:', err);
    useStore.getState().setLoadingMore(targetPath, false);
  }
}

// ══════════════════════════════════════════════════════
// Session 列表
// ══════════════════════════════════════════════════════

export async function loadSessions(): Promise<void> {
  try {
    const raw = await loomRpc('session.list');
    // session.list returns { sessions: [...] } — extract the array
    const sessions: Session[] = Array.isArray(raw) ? raw : (raw?.sessions || []);

    const s = useStore.getState();
    useStore.setState({ sessions });

    if (sessions.length > 0 && !s.currentSessionPath && !s.pendingNewSession && !s.pendingSessionSwitchPath) {
      // 首次加载：走完整的 switchSession 确保后端同步 + 消息加载
      await switchSession(sessions[0].path);
    }
  } catch { /* ignore */ }
}

// ══════════════════════════════════════════════════════
// Session 切换
// ══════════════════════════════════════════════════════

export async function switchSession(path: string): Promise<void> {
  const s = useStore.getState();
  const myVersion = ++_switchVersion;

  if (path === s.currentSessionPath && !s.pendingNewSession) {
    useStore.setState({ pendingSessionSwitchPath: null });
    return;
  }

  useStore.setState({ pendingSessionSwitchPath: path });

  // 关闭浮动面板
  const activePanel = useStore.getState().activePanel;
  if (activePanel === 'activity' || activePanel === 'automation') {
    useStore.getState().setActivePanel(null);
  }

  try {
    const data = await loomRpc('session.switch', { path, currentSessionPath: s.currentSessionPath });
    if (!isCurrentSwitch(myVersion, path)) return;

    const state = useStore.getState();

    // 同步 streamingSessions：切入的 session 可能正在 streaming
    let streamingSessions = state.streamingSessions;
    if (data.isStreaming && path) {
      if (!streamingSessions.includes(path)) {
        streamingSessions = [...streamingSessions, path];
      }
    }

    // 同步全局 agent 上下文
    const switchedAgent = data.agentId && data.agentId !== state.currentAgentId;
    const agentPatch: Record<string, any> = {};

    if (switchedAgent) {
      const ag = state.agents.find((a: any) => a.id === data.agentId);
      agentPatch.currentAgentId = data.agentId;
      agentPatch.agentName = data.agentName || ag?.name || data.agentId;
      agentPatch.agentYuan = ag?.yuan || 'hanako';
      agentPatch.agentAvatarUrl = ag?.hasAvatar ? `http://127.0.0.1:${getEnginePort()}/api/agents/${data.agentId}/avatar?t=${Date.now()}` : null;
    }

    // 保存当前 session 的附件到 keyed store
    const currentPath = s.currentSessionPath;
    const currentAttachments = state.attachedFiles;
    if (currentPath) {
      useStore.setState(prev => ({
        attachedFilesBySession: { ...prev.attachedFilesBySession, [currentPath]: [...currentAttachments] },
      }));
    }

    // 批量更新 store（切 currentSessionPath 切换对话内容；可见 desk/preview 状态由 workspace 激活流程恢复）
    useStore.setState({
      currentSessionPath: path,
      pendingSessionSwitchPath: null,
      pendingNewSession: false,
      selectedFolder: null,
      workspaceFolders: Array.isArray(data.workspaceFolders) ? data.workspaceFolders : [],
      selectedAgentId: null,
      welcomeVisible: false,
      memoryEnabled: data.memoryEnabled !== false,
      streamingSessions,
      attachedFiles: state.attachedFilesBySession[path] || [],
      deskContextAttached: false,
      docContextAttached: false,
      ...agentPatch,
    });

    await resetDeskForSessionCwd(data.cwd || null);
    if (myVersion !== _switchVersion) return;

    // 同步浏览器状态到 keyed store（服务端返回当前 session 的 browser 状态）
    if (path) {
      updateKeyed('browserBySession', path, {
        running: !!data.browserRunning,
        url: data.browserUrl || null,
        thumbnail: data.browserRunning ? (state.browserBySession[path]?.thumbnail ?? null) : null,
      });
    }

    useStore.getState().clearQuotedSelection();

    // Sync plan mode for the switched-to session
    window.dispatchEvent(new CustomEvent('hana-plan-mode', {
      detail: {
        enabled: data.permissionMode === 'read_only' || data.accessMode === 'read_only' || data.planMode === true,
        mode: data.permissionMode || data.accessMode,
      },
    }));
    if (data.thinkingLevel) {
      useStore.getState().setThinkingLevel(data.thinkingLevel);
    }

    // 刷新模型列表（当前 session 的模型可能不同）
    loadModels();

    // Hydrate per-session model snapshot from switch response。
    // provider 缺失不写入——空 provider 会让 ModelSelector 的复合键匹配全错
    // （老 session 的 meta 可能没带 provider，走 migration 或下一次显式选择修复）。
    if (data.currentModelId && data.currentModelProvider) {
      useStore.getState().updateSessionModel(path, {
        id: data.currentModelId,
        name: data.currentModelName || data.currentModelId,
        provider: data.currentModelProvider,
        input: Array.isArray(data.currentModelInput) ? data.currentModelInput : undefined,
        video: data.currentModelVideo ?? undefined,
        videoTransport: data.currentModelVideoTransport ?? undefined,
        videoTransportSupported: data.currentModelVideoTransportSupported ?? undefined,
        reasoning: data.currentModelReasoning ?? undefined,
        xhigh: data.currentModelXhigh ?? undefined,
        contextWindow: data.currentModelContextWindow ?? undefined,
      });
    }

    // 如果 store 中没有该 session 的消息数据，加载之
    const hasData = !!useStore.getState().chatSessions?.[path];
    if (!hasData) {
      await loadMessages(path);
      if (myVersion !== _switchVersion) return;
    }

    // 切换会话后刷新 context ring
    useStore.setState({ contextTokens: null, contextWindow: null, contextPercent: null });
    window.openloom?.send('context_usage', { sessionPath: path }).then((result: any) => {
      if (myVersion !== _switchVersion) return;
      if (result && typeof result.used === 'number') {
        useStore.setState({
          contextTokens: result.used,
          contextWindow: result.total ?? null,
          contextPercent: result.percent ?? null,
        });
      }
    });

    // Restore input focus only if the user is still in the chat surface that initiated the switch.
    requestChatInputFocus(path);
  } catch (err) {
    if (myVersion !== _switchVersion) return;
    useStore.setState((state: Record<string, any>) => (
      state.pendingSessionSwitchPath === path ? { pendingSessionSwitchPath: null } : {}
    ));
    console.error('[session] switch failed:', err);
    showSessionSwitchError(path, errorMessage(err));
  }
}

// ══════════════════════════════════════════════════════
// 新建 Session
// ══════════════════════════════════════════════════════

export async function createNewSession(): Promise<void> {
  // Entering the pending new-session workspace is a navigation boundary.
  // Any in-flight switchSession response now belongs to the previous view.
  invalidateSessionSwitches();

  // 关闭浮动面板
  if (useStore.getState().activePanel === 'activity') {
    useStore.getState().setActivePanel(null);
  }

  const s = useStore.getState();
  const defaultFolder = s.homeFolder || s.deskBasePath || null;

  useStore.setState({
    welcomeVisible: true,
    currentSessionPath: null,
    pendingSessionSwitchPath: null,
    // 有显式 Agent home 时以 home 为准；没有绑定 workspace 的 agent
    // 以当前 session cwd 延续工作流，不从其他 agent 的 home_folder 推导。
    selectedFolder: defaultFolder,
    workspaceFolders: [],
    selectedAgentId: null,
    pendingNewSession: true,
    attachedFiles: [],
    deskContextAttached: false,
    docContextAttached: false,
  });

  await activateWorkspaceDesk(defaultFolder);

  // 重置 context ring
  useStore.setState({ contextTokens: null, contextWindow: null, contextPercent: null });
  try {
    const data = await loomRpc('session.permission_mode');
    const mode = data.defaultMode || data.mode || 'ask';
    window.dispatchEvent(new CustomEvent('hana-plan-mode', {
      detail: { enabled: mode === 'read_only', mode },
    }));
  } catch {
    window.dispatchEvent(new CustomEvent('hana-plan-mode', { detail: { enabled: false, mode: 'ask' } }));
  }

  // pending 状态下刷新 model 列表，让 ModelSelector 显示 agent Chat 默认 model
  loadModels();

  requestChatInputFocus(null);
}

// ══════════════════════════════════════════════════════
// 确保 Session 存在（首次发消息时调用）
// ══════════════════════════════════════════════════════

export async function ensureSession(): Promise<boolean> {
  const s = useStore.getState();
  if (!s.pendingNewSession) return true;

  try {
    const body: Record<string, any> = { memoryEnabled: s.memoryEnabled };
    if (s.selectedFolder) {
      body.cwd = s.selectedFolder;
    }
    if (s.workspaceFolders?.length) {
      body.workspaceFolders = s.workspaceFolders;
    }
    if (s.selectedAgentId && s.selectedAgentId !== s.currentAgentId) {
      body.agentId = s.selectedAgentId;
    }
    body.currentSessionPath = s.currentSessionPath;

    const data = await loomRpc('session.create', body);

    const justSelected = s.selectedFolder;

    // 基础状态更新
    const patch: Record<string, any> = {
      pendingNewSession: false,
      pendingSessionSwitchPath: null,
      selectedFolder: null,
      workspaceFolders: Array.isArray(data.workspaceFolders) ? data.workspaceFolders : [],
      selectedAgentId: null,
    };

    if (data.agentId) {
      const switched = data.agentId !== s.currentAgentId;
      patch.currentAgentId = data.agentId;
      if (data.agentName) patch.agentName = data.agentName;
      if (switched) {
        const ag = s.agents.find((a: any) => a.id === data.agentId);
        if (ag?.yuan) patch.agentYuan = ag.yuan;
        patch.agentAvatarUrl = null;
        window.i18n.defaultName = data.agentName || s.agentName;
        // 异步刷新头像
        loomRpc('system.health').then((d: any) => {
          loadAvatarsAction(d.avatars);
        }).catch(() => {
          loadAvatarsAction();
        });
      }
    }

    if (data.path) {
      patch.currentSessionPath = data.path;
      // 初始化空 session，ChatArea 自动渲染
      useStore.getState().initSession(data.path, [], false);
    }

    useStore.setState(patch);
    if (data.thinkingLevel) {
      useStore.getState().setThinkingLevel(data.thinkingLevel);
    }

    await resetDeskForSessionCwd(data.cwd || null);

    window.dispatchEvent(new CustomEvent('hana-plan-mode', {
      detail: {
        enabled: data.permissionMode === 'read_only' || data.accessMode === 'read_only' || data.planMode === true,
        mode: data.permissionMode || data.accessMode,
      },
    }));

    // Persist the model snapshot for the newly created session so
    // ModelSelector and InputArea surface the correct active model.
    if (data.path && data.currentModelId) {
      useStore.getState().updateSessionModel(data.path, {
        id: data.currentModelId,
        provider: data.currentModelProvider || '',
        name: data.currentModelName || data.currentModelId,
      });
    }

    await loadSessions();

    // 刷新模型列表：session 创建后 activeModel 已绑定，需要同步到 UI
    loadModels();

    // 更新 cwdHistory
    if (justSelected) {
      const currentState = useStore.getState();
      let cwdHistory = currentState.cwdHistory.filter((p: string) => p !== justSelected);
      cwdHistory = [justSelected, ...cwdHistory];
      if (cwdHistory.length > 10) cwdHistory = cwdHistory.slice(0, 10);
      useStore.setState({ cwdHistory });
    }

    return true;
  } catch (err) {
    console.error('[session] create failed:', err);
    showSessionCreationError(errorMessage(err));
    return false;
  }
}

// ══════════════════════════════════════════════════════
// 归档 Session
// ══════════════════════════════════════════════════════

export async function archiveSession(path: string): Promise<void> {
  try {
    await loomRpc('session.archive', { path });

    const s = useStore.getState();
    const isCurrent = path === s.currentSessionPath;
    clearSessionRuntimeCaches(path);

    // 乐观地从 store 里删掉这条 session，避免 loadSessions 回来前 UI 还显示它
    const remainingSessions = s.sessions.filter((sess: any) => sess.path !== path);
    useStore.setState({ sessions: remainingSessions });

    if (isCurrent) {
      clearChatAction();
      // 先把 currentSessionPath 设为 null，但同时设 pendingNewSession 防止
      // loadSessions 内部的自动 switchSession 逻辑抢先执行
      useStore.setState({ currentSessionPath: null, pendingNewSession: true });
    }

    // 重新拉最新 session 列表（不会自动 switch，因为 pendingNewSession=true）
    await loadSessions();

    // 还原 pendingNewSession，然后决定跳到哪
    useStore.setState({ pendingNewSession: false });
    const updated = useStore.getState();
    if (updated.sessions.length === 0) {
      await createNewSession();
    } else if (!updated.currentSessionPath) {
      await switchSession(updated.sessions[0].path);
    }
  } catch (err) {
    console.error('[session] archive failed:', err);
    showSidebarToast(window.t('session.archiveFailed'));
  }
}

export async function deleteMessage(sessionPath: string, messageId: string, dbId: number): Promise<boolean> {
  try {
    const data = await loomRpc('session.delete_message', {
      session_id: sessionPath,
      msg_id: dbId,
    });
    if (data?.ok) {
      useStore.getState().removeMessage(sessionPath, messageId);
      return true;
    }
    return false;
  } catch (err) {
    console.error('[session] delete message failed:', err);
    return false;
  }
}

// ══════════════════════════════════════════════════════
// 归档管理：列出 / 恢复 / 永久删 / 批量清理
// ══════════════════════════════════════════════════════

export interface ArchivedSession {
  path: string;
  title: string | null;
  archivedAt: string;
  sizeBytes: number;
  agentId: string;
  agentName: string;
}

export type RestoreResult = 'ok' | 'conflict' | 'error';

export async function listArchivedSessions(): Promise<ArchivedSession[]> {
  try {
    const data = await loomRpc('session.archived_list');
    return data?.archived || [];
  } catch (err) {
    console.error('[archived] list failed:', err);
    return [];
  }
}

export async function restoreSession(path: string): Promise<RestoreResult> {
  try {
    await loomRpc('session.restore', { path });
    return 'ok';
  } catch (err: any) {
    console.error('[archived] restore failed:', err);
    if (err?.message?.includes('conflict') || err?.code === 409) return 'conflict';
    return 'error';
  }
}

export async function deleteArchivedSession(path: string): Promise<boolean> {
  try {
    await loomRpc('session.delete_archived', { path });
    return true;
  } catch (err) {
    console.error('[archived] delete failed:', err);
    return false;
  }
}

export async function cleanupArchivedSessions(maxAgeDays: 30 | 90): Promise<{ deleted: number }> {
  try {
    const data = await loomRpc('session.cleanup', { maxAgeDays });
    return { deleted: data?.deleted ?? 0 };
  } catch (err) {
    console.error('[archived] cleanup failed:', err);
    return { deleted: 0 };
  }
}

// ══════════════════════════════════════════════════════
// 重命名 Session
// ══════════════════════════════════════════════════════

export async function renameSession(path: string, title: string): Promise<boolean> {
  try {
    await loomRpc('session.rename', { path, title });
    // 乐观更新 store 中的 title
    const sessions = useStore.getState().sessions.map(s =>
      s.path === path ? { ...s, title } : s,
    );
    useStore.setState({ sessions });
    return true;
  } catch (err) {
    console.error('[session] rename failed:', err);
    return false;
  }
}

// ══════════════════════════════════════════════════════
// 置顶 / 取消置顶 Session
// ══════════════════════════════════════════════════════

export async function pinSession(path: string, pinned: boolean): Promise<boolean> {
  try {
    const data = await loomRpc('session.pin', { path, pinned });

    const pinnedAt = typeof data.pinnedAt === 'string' ? data.pinnedAt : null;
    const sessions = useStore.getState().sessions.map(s =>
      s.path === path ? { ...s, pinnedAt } : s,
    );
    useStore.setState({ sessions });
    return true;
  } catch (err) {
    console.error('[session] pin failed:', err);
    showSidebarToast(window.t(pinned ? 'session.pinFailed' : 'session.unpinFailed'));
    return false;
  }
}

// ══════════════════════════════════════════════════════
// Toast
// ══════════════════════════════════════════════════════

export function showSidebarToast(text: string, duration = 3000): void {
  useStore.getState().addToast(text, 'info', duration);
}

function tr(key: string): string {
  return typeof window !== 'undefined' && typeof window.t === 'function'
    ? window.t(key)
    : key;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err || 'Unknown error');
}

function showSessionCreationError(detail: unknown): void {
  const label = tr('session.createFailed');
  const message = `${label}: ${errorMessage(detail)}`;
  const state = useStore.getState();
  state.setInlineError?.(state.currentSessionPath || '', message, 6000);
  state.addToast(message, 'error', 6000);
}

function showSessionSwitchError(targetPath: string, detail: unknown): void {
  const label = tr('session.switchFailed');
  const message = `${label}: ${errorMessage(detail)}`;
  const state = useStore.getState();
  state.setInlineError?.(state.currentSessionPath || targetPath || '', message, 6000);
  state.addToast(message, 'error', 6000);
}
