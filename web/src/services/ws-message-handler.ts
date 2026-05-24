/**
 * ws-message-handler.ts — WebSocket 消息分发
 *
 * 处理从 openLoom 后端收到的 JSON-RPC 通知和响应，
 * 将聊天消息、Agent 状态变化等事件分发到 Zustand store。
 */
import { useStore } from '../stores';
import type { ContentBlock } from '../stores/chat-types';
import { updateKeyed } from '../stores/create-keyed-slice';

// Track streaming assistant message per session
const streamingMessages: Record<string, { id: string; text: string; blocks: ContentBlock[] }> = {};

/** 处理从 WebSocket 收到的服务端消息 */
export function handleServerMessage(msg: any): void {
  if (!msg || typeof msg !== 'object') return;

  // Raw type-based messages (non-JSON-RPC) from ws.rs raw handler
  if (msg.type === 'context_usage') {
    const path = msg.sessionPath;
    if (path) {
      updateKeyed('contextBySession', path, {
        tokens: msg.tokens ?? null,
        window: msg.contextWindow ?? null,
        percent: msg.percent ?? null,
      }, (s: any, d: any) => ({
        contextTokens: d.tokens,
        contextWindow: d.window,
        contextPercent: d.percent,
      }));
    }
    return;
  }

  if (msg.type === 'compaction_end') {
    const path = msg.sessionPath;
    if (path) {
      useStore.getState().removeCompactingSession(path);
      updateKeyed('contextBySession', path, {
        tokens: msg.tokens ?? null,
        window: msg.contextWindow ?? null,
        percent: msg.percent ?? null,
      }, (s: any, d: any) => ({
        contextTokens: d.tokens,
        contextWindow: d.window,
        contextPercent: d.percent,
      }));
    }
    return;
  }

  // JSON-RPC notification: { jsonrpc: "2.0", method: "...", params: {...} }
  if (msg.jsonrpc === '2.0' && msg.method) {
    handleNotification(msg.method, msg.params);
    return;
  }

  // JSON-RPC response: { jsonrpc: "2.0", result: {...}, id: N }
  if (msg.jsonrpc === '2.0' && msg.result !== undefined) {
    handleResponse(msg.id, msg.result);
    return;
  }
}

/** 更新 streaming 相关状态到 store */
export function applyStreamingStatus(_state: any): void {}

// ── 内部分发 ──

function getOrCreateStreamingMessage(sessionPath: string): { id: string; text: string; blocks: ContentBlock[] } {
  if (!streamingMessages[sessionPath]) {
    streamingMessages[sessionPath] = {
      id: `assistant-${Date.now()}`,
      text: '',
      blocks: [{ type: 'text', html: '', source: '' }],
    };
  }
  return streamingMessages[sessionPath];
}

function handleNotification(method: string, params: any): void {
  switch (method) {
    // Stream delta — partial token from the model
    case 'chat.stream_delta': {
      const { currentSessionPath } = useStore.getState();
      const sid = params?.session_id || currentSessionPath;
      if (!sid) return;
      const delta = params?.delta || '';
      // Filter internal control signals that may leak into the stream
      const cleanDelta = delta
        .replace(/\x00USAGE:\d+:\d+:\d+/g, '')
        .replace(/USAGE:\d+:\d+:\d+/g, '');
      if (!cleanDelta) return;

      // Ensure the session is initialized in the store before we can appendItem/updateMessageById
      if (!useStore.getState().chatSessions[sid]) {
        useStore.getState().initSession(sid, [], false);
      }
      // Hide welcome screen when we start receiving content
      const existingSessions = useStore.getState().streamingSessions || [];
      if (!existingSessions.includes(sid)) {
        useStore.setState({
          welcomeVisible: false,
          isStreaming: true,
          streamingSessions: [...existingSessions, sid],
        });
      } else {
        useStore.setState({ welcomeVisible: false, isStreaming: true });
      }

      const isFirst = !streamingMessages[sid];
      const streaming = getOrCreateStreamingMessage(sid);
      streaming.text += cleanDelta;

      // Update the message in the store
      import('../utils/markdown').then(({ renderMarkdown }) => {
        const html = renderMarkdown(streaming.text);
        streaming.blocks = [{ type: 'text', html, source: streaming.text }];
        const msgData = {
          id: streaming.id,
          role: 'assistant' as const,
          text: streaming.text,
          textHtml: html,
          blocks: streaming.blocks,
          timestamp: Date.now(),
          isStreaming: true,
        };
        if (isFirst) {
          // First delta: append a new streaming message
          useStore.getState().appendItem(sid, { type: 'message', data: msgData });
        } else {
          // Subsequent deltas: update the existing streaming message in-place
          const updated = useStore.getState().updateMessageById(sid, streaming.id, () => msgData);
          if (!updated) {
            // Fallback: if somehow not found, append
            useStore.getState().appendItem(sid, { type: 'message', data: msgData });
          }
        }
      });
      break;
    }

    // Stream end — full response complete
    case 'chat.stream_end': {
      const { currentSessionPath } = useStore.getState();
      const sid = params?.session_id || currentSessionPath;
      if (!sid) return;

      // Ensure session initialized
      if (!useStore.getState().chatSessions[sid]) {
        useStore.getState().initSession(sid, [], false);
      }

      const streaming = streamingMessages[sid];
      // Use the full_response from event if available (more authoritative than accumulated deltas)
      // Strip any internal control signals (\x00USAGE:... or USAGE:...) from the response text.
      const rawText = params?.response || params?.full_response || streaming?.text || '';
      const fullText = rawText
        .replace(/\x00USAGE:\d+:\d+:\d+/g, '')   // "\x00USAGE:689:484:0"
        .replace(/USAGE:\d+:\d+:\d+/g, '')        // "USAGE:689:484:0" (no prefix)
        .trimEnd();

      if (fullText) {
        import('../utils/markdown').then(({ renderMarkdown }) => {
          const html = renderMarkdown(fullText);
          const finalMsg = {
            id: streaming?.id || `assistant-${Date.now()}`,
            role: 'assistant' as const,
            text: fullText,
            textHtml: html,
            blocks: [{ type: 'text' as const, html, source: fullText }],
            timestamp: Date.now(),
            isStreaming: false,
          };
          if (streaming) {
            const updated = useStore.getState().updateMessageById(sid, streaming.id, () => finalMsg);
            if (!updated) {
              useStore.getState().appendItem(sid, { type: 'message', data: finalMsg });
            }
          } else {
            useStore.getState().appendItem(sid, { type: 'message', data: finalMsg });
          }
          // Delete AFTER the store update so late-arriving stream_delta events
          // see streamingMessages[sid] as still set and don't create a new message.
          delete streamingMessages[sid];
        });
      } else {
        delete streamingMessages[sid];
      }
      useStore.setState({ isStreaming: false, streamingSessions: [] });
      break;
    }

    // 聊天回复（ws.rs 在收到 type:'prompt' 后发出）
    case 'chat.response': {
      const { currentSessionPath } = useStore.getState();
      if (!currentSessionPath) return;
      const response = params?.response || '';
      if (!response) return;
      import('../utils/markdown').then(({ renderMarkdown }) => {
        useStore.getState().appendItem(currentSessionPath, {
          type: 'message',
          data: {
            id: `assistant-${Date.now()}`,
            role: 'assistant',
            text: response,
            textHtml: renderMarkdown(response),
            blocks: [{ type: 'text', html: renderMarkdown(response), source: response }],
            timestamp: Date.now(),
          },
        });
      });
      useStore.setState({ isStreaming: false, streamingSessions: [] });
      break;
    }

    case 'chat.steer': {
      break;
    }

    // ── 工具调用状态 ──
    case 'tool.start': {
      const sid = params?.session_id;
      if (!sid) return;
      const name = params?.name || 'unknown';
      const args = params?.args || {};

      // Ensure session and streaming message exist
      if (!useStore.getState().chatSessions[sid]) {
        useStore.getState().initSession(sid, [], false);
      }
      const streaming = getOrCreateStreamingMessage(sid);

      // Find or create tool_group block
      let toolGroup = streaming.blocks.find(b => b.type === 'tool_group') as
        { type: 'tool_group'; tools: Array<{ name: string; args?: Record<string, unknown>; done: boolean; success: boolean; details?: Record<string, unknown> }>; collapsed: boolean } | undefined;
      if (!toolGroup) {
        toolGroup = { type: 'tool_group', tools: [], collapsed: false };
        streaming.blocks = [toolGroup, ...streaming.blocks.filter(b => b.type !== 'tool_group')];
      }

      toolGroup.tools.push({ name, args, done: false, success: false });

      // Update store
      const msgData = {
        id: streaming.id,
        role: 'assistant' as const,
        text: streaming.text,
        blocks: streaming.blocks,
        timestamp: Date.now(),
        isStreaming: true,
      };
      const existing = useStore.getState().chatSessions[sid]?.items?.find(
        (item: any) => item.type === 'message' && item.data?.id === streaming.id,
      );
      if (existing) {
        useStore.getState().updateMessageById(sid, streaming.id, () => msgData);
      } else {
        useStore.getState().appendItem(sid, { type: 'message', data: msgData });
      }
      break;
    }

    case 'tool.end': {
      const sid = params?.session_id;
      if (!sid) return;
      const name = params?.name || 'unknown';
      const success = !!params?.success;
      const details = params?.details || {};

      const streaming = streamingMessages[sid];
      if (!streaming) return;

      // Find the matching unfinished tool (from back to front)
      const toolGroup = streaming.blocks.find(b => b.type === 'tool_group') as
        { type: 'tool_group'; tools: Array<{ name: string; args?: Record<string, unknown>; done: boolean; success: boolean; details?: Record<string, unknown> }>; collapsed: boolean } | undefined;
      if (!toolGroup) return;

      for (let i = toolGroup.tools.length - 1; i >= 0; i--) {
        const t = toolGroup.tools[i];
        if (t.name === name && !t.done) {
          t.done = true;
          t.success = success;
          t.details = { ...t.details, ...details };
          break;
        }
      }

      // Auto-collapse if all tools done and more than one
      if (toolGroup.tools.length > 1 && toolGroup.tools.every(t => t.done)) {
        toolGroup.collapsed = true;
      }

      // Update store
      const msgData = {
        id: streaming.id,
        role: 'assistant' as const,
        text: streaming.text,
        blocks: streaming.blocks,
        timestamp: Date.now(),
        isStreaming: true,
      };
      useStore.getState().updateMessageById(sid, streaming.id, () => msgData);
      break;
    }

    // Agent 状态变化
    case 'agent.state_changed': {
      const newState = params?.new_state;
      if (newState === 'idle') {
        useStore.setState({ isStreaming: false });
      } else if (newState === 'thinking' || newState === 'acting') {
        useStore.setState({ isStreaming: true });
      }
      break;
    }

    // Token 用量
    case 'token.usage': {
      // Update context ring with real token counts from the inference layer
      const promptTokens: number = params?.prompt_tokens ?? 0;
      const contextWindow: number | null = params?.context_window ?? null;
      if (promptTokens > 0) {
        const { currentSessionPath } = useStore.getState();
        const sid = params?.session_id || currentSessionPath;
        // Fetch model context window if not provided
        if (contextWindow) {
          const percent = Math.min(100, (promptTokens / contextWindow) * 100);
          useStore.setState({
            contextTokens: promptTokens,
            contextWindow,
            contextPercent: percent,
          });
        } else {
          // No context window in event — just update used tokens, keep existing window
          const existingWindow = useStore.getState().contextWindow;
          if (existingWindow && existingWindow > 0) {
            const percent = Math.min(100, (promptTokens / existingWindow) * 100);
            useStore.setState({
              contextTokens: promptTokens,
              contextPercent: percent,
            });
          } else {
            useStore.setState({ contextTokens: promptTokens });
          }
          // Refresh full context usage from backend to get accurate window size
          if (sid) {
            import('../adapter').then(({ loomRpc }) => {
              loomRpc('context_usage', { sessionPath: sid }).then((r: any) => {
                if (r && typeof r.used === 'number') {
                  useStore.setState({
                    contextTokens: r.used,
                    contextWindow: r.total ?? null,
                    contextPercent: r.percent ?? null,
                  });
                }
              }).catch(() => {});
            });
          }
        }
      }
      break;
    }

    // 错误
    case 'error': {
      const message = params?.message || 'Unknown error';
      console.error('[ws] engine error:', message);
      useStore.getState().addToast?.(message, 'error');
      useStore.setState({ isStreaming: false });
      break;
    }

    // 认知更新（静默处理）
    case 'cognition.updated': {
      break;
    }

    // 心跳
    case 'heartbeat.tick': {
      break;
    }

    case 'models-changed': {
      import('../utils/ui-helpers').then(({ loadModels }) => loadModels());
      break;
    }

    // 权限请求
    case 'permission.required': {
      break;
    }

    default: {
      break;
    }
  }
}

function handleResponse(id: number, result: any): void {
  // JSON-RPC response for request with matching id
  // These are handled by the loomRpc promise resolution in preload.js
  // No action needed here
}
