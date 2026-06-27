import type { IMStore } from './imStore'
import type { IMMessage, InstanceConfig } from './types'

/** Help message sent on first contact (direct sendMessage — no token cost). */
export const HELP_MESSAGE = [
  '你好！我是 openLoom Agent，已连接到你的微信。',
  '',
  '你可以直接发消息和我聊天，也可以使用以下斜杠命令：',
  '  • /new — 开始新会话（清除当前上下文）',
  '  • /mode <operate|ask|readonly|plan> — 切换权限模式',
  '  • /cd <路径> — 切换当前会话的工作目录',
  '  • /model [名称] — 查看/切换模型（全局生效）',
  '  • /agent [名称] — 查看/切换当前会话的 Agent',
  '  • /memory <on|off> — 开启/关闭会话记忆',
  '  • /compact — 压缩当前会话上下文',
  '  • /stop — 停止当前正在生成的回复',
  '  • /help — 显示本帮助',
].join('\n')

/** Map user-facing mode aliases to engine permission_mode values. */
const MODE_ALIASES: Record<string, string> = {
  operate: 'operate',
  ask: 'ask',
  readonly: 'read_only',
  read_only: 'read_only',
  plan: 'plan',
}

/**
 * ImBridge — bridges incoming IM messages to the Rust agent engine and sends
 * the agent's reply back through the originating IM channel.
 *
 * - Connects to the engine via WebSocket (same JSON-RPC the renderer uses).
 * - One agent session per IM conversation (private=senderId, group=groupId),
 *   persisted in `im_conversations`. The engine session id (a UUID returned by
 *   `session.create`) is stored as `coworkSessionId`; `/new` starts a fresh one.
 * - Per-conversation permission mode is held in `convState` and applied on each
 *   `chat.send`. Model/agent/workspace/memory changes are issued to the engine
 *   directly via their respective RPCs.
 */
export class ImBridge {
  private imStore: IMStore
  private onSessionCreated?: () => void
  private ws: WebSocket | null = null
  private port: number | null = null
  private nextId = 1
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> }>()
  /** Per-engine-session IM state (currently just the permission mode). */
  private convState = new Map<string, { permissionMode: string }>()

  constructor(imStore: IMStore, onSessionCreated?: () => void) {
    this.imStore = imStore
    this.onSessionCreated = onSessionCreated
  }

  connect(port: number): void {
    this.port = port
    this.doConnect()
  }

  disconnect(): void {
    this.port = null
    if (this.ws) {
      try { this.ws.close() } catch { /* ignore */ }
      this.ws = null
    }
    for (const [, entry] of this.pending) {
      clearTimeout(entry.timer)
      entry.reject(new Error('ImBridge disconnected'))
    }
    this.pending.clear()
  }

  private doConnect(): void {
    if (!this.port) return
    const url = `ws://127.0.0.1:${this.port}/ws`
    let ws: WebSocket
    try {
      ws = new WebSocket(url)
    } catch (e) {
      console.error('[ImBridge] WS construct failed, retry in 2s:', e)
      setTimeout(() => this.doConnect(), 2000)
      return
    }
    this.ws = ws

    ws.addEventListener('open', () => {
      console.log('[ImBridge] connected to engine:', url)
    })
    ws.addEventListener('close', () => {
      console.warn('[ImBridge] WS closed, reconnect in 2s')
      this.ws = null
      if (this.port) setTimeout(() => this.doConnect(), 2000)
    })
    ws.addEventListener('error', (e) => {
      console.error('[ImBridge] WS error:', e)
    })
    ws.addEventListener('message', (ev) => {
      let msg: any
      try { msg = JSON.parse(ev.data as string) } catch { return }
      if (msg.id != null && this.pending.has(msg.id)) {
        const entry = this.pending.get(msg.id)!
        this.pending.delete(msg.id)
        clearTimeout(entry.timer)
        if (msg.error) entry.reject(new Error(msg.error?.message ?? 'RPC error'))
        else entry.resolve(msg.result)
      }
      // chat.* streaming notifications are ignored — we wait for the full response.
    })
  }

  rpc(method: string, params: Record<string, unknown> = {}): Promise<any> {
    return new Promise((resolve, reject) => {
      const ws = this.ws
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        reject(new Error(`ImBridge WS not connected [${method}]`))
        return
      }
      const id = this.nextId++
      const timer = setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id)
          reject(new Error(`ImBridge RPC timeout: ${method}`))
        }
      }, method === 'chat.send' ? 1_800_000 : 30_000)
      this.pending.set(id, { resolve, reject, timer })
      try {
        ws.send(JSON.stringify({ jsonrpc: '2.0', method, params, id }))
      } catch (e: any) {
        this.pending.delete(id)
        clearTimeout(timer)
        reject(new Error(`ImBridge WS send failed: ${e?.message ?? e}`))
      }
    })
  }

  /** Current permission mode for a session (defaults to "operate"). */
  private getMode(sessionId: string): string {
    return this.convState.get(sessionId)?.permissionMode || 'operate'
  }

  /** Bound engine session id for a conversation, if any. */
  private getSid(config: InstanceConfig, conversationId: string): string | undefined {
    return this.imStore.getConversation(config.instanceId, conversationId)?.coworkSessionId
  }

  /**
   * Handle an incoming IM message: route to the bound agent session and reply.
   * `reply` sends a text back through the originating channel (the real user
   * who sent the message — direct chat = senderId, group = groupId).
   */
  async handleMessage(
    msg: IMMessage,
    config: InstanceConfig,
    reply: (text: string) => Promise<void>,
  ): Promise<void> {
    const { platform, conversationId, content } = msg
    const trimmed = content.trim()
    if (!trimmed) return // ignore empty messages

    // Slash commands — handled locally, never forwarded to the agent.
    if (trimmed.startsWith('/')) {
      if (await this.handleCommand(trimmed, msg, config, reply)) return
    }

    let sessionId = this.getSid(config, conversationId)
    let isFirstContact = false
    if (!sessionId) {
      // First contact: let the engine create the session (returns the real
      // UUID id, persisted + visible in the desktop sidebar), bind the agent,
      // and send a help message. Then fall through to actually answer this
      // first message so the conversation has a record from the start.
      isFirstContact = true
      try {
        const created: any = await this.rpc('session.create', {})
        sessionId = created.session_id
        this.imStore.upsertConversation(platform, config.instanceId, conversationId, sessionId!)
        this.onSessionCreated?.()
      } catch (e: any) {
        console.warn('[ImBridge] session.create failed:', e?.message)
        await reply(`❌ 创建会话失败: ${e?.message ?? e}`)
        return
      }
      const agentConfigName = config.agentId || 'default'
      try {
        await this.rpc('session.bind_agent', { session_id: sessionId, agent_config_name: agentConfigName })
      } catch (e: any) {
        console.warn(`[ImBridge] bind_agent '${agentConfigName}' failed (using default):`, e?.message)
      }
      await reply(HELP_MESSAGE)
      // fall through: send the user's opening line to the agent too
    }

    try {
      const result: any = await this.rpc('chat.send', {
        session_id: sessionId!,
        content,
        permission_mode: this.getMode(sessionId!),
      })
      const responseText = (result?.response ?? '').toString().trim()
      if (responseText) {
        await reply(responseText)
      }
      if (isFirstContact) {
        // Auto-title the new session so the desktop sidebar shows something
        // meaningful instead of an empty name. Non-critical, fire-and-forget.
        this.rpc('session.auto_title', { session_id: sessionId })
          .then((r: any) => { if (r?.title) this.onSessionCreated?.() })
          .catch(() => { /* ignore */ })
      }
    } catch (e: any) {
      console.error('[ImBridge] chat.send failed:', e?.message)
      await reply(`❌ 处理失败: ${e?.message ?? e}`)
    }
  }

  /**
   * Dispatch a slash command. Returns true if `trimmed` was recognized as a
   * command (and a reply was sent); false lets handleMessage treat the text as
   * a normal message.
   */
  private async handleCommand(
    trimmed: string,
    msg: IMMessage,
    config: InstanceConfig,
    reply: (text: string) => Promise<void>,
  ): Promise<boolean> {
    const { platform, conversationId } = msg

    // /help — show the command list
    if (trimmed === '/help' || trimmed === '/帮助' || trimmed === '/?') {
      await reply(HELP_MESSAGE)
      return true
    }

    // /new — start a fresh engine session for this conversation
    if (trimmed === '/new' || trimmed.startsWith('/new ') || trimmed === '/新会话') {
      try {
        const created: any = await this.rpc('session.create', {})
        const newSid: string = created.session_id
        this.imStore.upsertConversation(platform, config.instanceId, conversationId, newSid)
        this.convState.delete(newSid) // new session resets to default mode
        const agentConfigName = config.agentId || 'default'
        try {
          await this.rpc('session.bind_agent', { session_id: newSid, agent_config_name: agentConfigName })
        } catch (e: any) {
          console.warn(`[ImBridge] bind_agent '${agentConfigName}' failed:`, e?.message)
        }
        this.onSessionCreated?.()
        await reply('✅ 已创建新会话')
      } catch (e: any) {
        await reply(`❌ 创建会话失败: ${e?.message ?? e}`)
      }
      return true
    }

    // /mode <operate|ask|readonly|plan> — set per-session permission mode
    if (trimmed === '/mode' || trimmed.startsWith('/mode ')) {
      const arg = trimmed.slice('/mode'.length).trim().toLowerCase()
      const sid = this.getSid(config, conversationId)
      if (!arg) {
        await reply(`当前权限模式：${sid ? this.getMode(sid) : 'operate'}\n可选：operate / ask / readonly / plan`)
        return true
      }
      const mode = MODE_ALIASES[arg]
      if (!mode) {
        await reply('⚠️ 未知模式，可选：operate / ask / readonly / plan')
        return true
      }
      if (sid) {
        const s = this.convState.get(sid) || { permissionMode: 'operate' }
        s.permissionMode = mode
        this.convState.set(sid, s)
      }
      await reply(`✅ 权限模式已切换为 ${arg}${mode === 'ask' ? '（⚠️ ask 模式下中高风险工具需确认，IM 端可能无法响应）' : ''}`)
      return true
    }

    // /cd <path> — set the session workspace
    if (trimmed === '/cd' || trimmed.startsWith('/cd ')) {
      const path = trimmed.slice('/cd'.length).trim()
      if (!path) {
        await reply('用法：/cd <工作目录路径>')
        return true
      }
      const sid = this.getSid(config, conversationId)
      if (!sid) {
        await reply('⚠️ 当前没有活动会话，先发条消息开始对话吧')
        return true
      }
      try {
        await this.rpc('workspace.set_session', { session_id: sid, path })
        await reply(`✅ 工作路径已切换为 ${path}`)
      } catch (e: any) {
        await reply(`❌ 切换路径失败: ${e?.message ?? e}`)
      }
      return true
    }

    // /model [name] — list or switch the active model (global)
    if (trimmed === '/model' || trimmed.startsWith('/model ')) {
      const arg = trimmed.slice('/model'.length).trim()
      if (!arg) {
        try {
          const r: any = await this.rpc('model.list', {})
          const models: any[] = r?.models || []
          const active = r?.activeModel || ''
          const lines = models.map((m) => `${m.name === active ? '▶ ' : '  '}${m.name}（${m.model}）`)
          await reply(`可用模型：\n${lines.join('\n') || '（无）'}\n当前：${active || '（未设置）'}\n切换：/model <名称>`)
        } catch (e: any) {
          await reply(`❌ 获取模型列表失败: ${e?.message ?? e}`)
        }
        return true
      }
      try {
        await this.rpc('model.switch', { model: arg })
        await reply(`✅ 模型已切换为 ${arg}（全局生效）`)
      } catch (e: any) {
        await reply(`❌ 切换模型失败: ${e?.message ?? e}`)
      }
      return true
    }

    // /agent [name] — list agent configs or rebind this session's agent
    if (trimmed === '/agent' || trimmed.startsWith('/agent ')) {
      const arg = trimmed.slice('/agent'.length).trim()
      if (!arg) {
        try {
          const r: any = await this.rpc('agent.config.list', {})
          const configs: any[] = r?.configs || []
          const lines = configs.map((c) => `  ${c.name}`)
          await reply(`可用 Agent：\n${lines.join('\n') || '（无）'}\n切换：/agent <名称>`)
        } catch (e: any) {
          await reply(`❌ 获取 Agent 列表失败: ${e?.message ?? e}`)
        }
        return true
      }
      const sid = this.getSid(config, conversationId)
      if (!sid) {
        await reply('⚠️ 当前没有活动会话，先发条消息开始对话吧')
        return true
      }
      try {
        await this.rpc('session.bind_agent', { session_id: sid, agent_config_name: arg })
        await reply(`✅ Agent 已切换为 ${arg}`)
      } catch (e: any) {
        await reply(`❌ 切换 Agent 失败: ${e?.message ?? e}`)
      }
      return true
    }

    // /memory <on|off> — toggle per-session memory recording
    if (trimmed === '/memory' || trimmed.startsWith('/memory ')) {
      const arg = trimmed.slice('/memory'.length).trim().toLowerCase()
      const sid = this.getSid(config, conversationId)
      if (!sid) {
        await reply('⚠️ 当前没有活动会话，先发条消息开始对话吧')
        return true
      }
      if (arg !== 'on' && arg !== 'off') {
        await reply('用法：/memory <on|off>')
        return true
      }
      const enabled = arg === 'on'
      try {
        await this.rpc('session.set_memory_enabled', { session_id: sid, enabled })
        await reply(`✅ 会话记忆已${enabled ? '开启' : '关闭'}`)
      } catch (e: any) {
        await reply(`❌ 切换记忆失败: ${e?.message ?? e}`)
      }
      return true
    }

    // /compact — summarize the session history to reclaim context
    if (trimmed === '/compact' || trimmed.startsWith('/compact ') || trimmed === '/压缩') {
      const sid = this.getSid(config, conversationId)
      if (!sid) {
        await reply('⚠️ 当前没有活动会话，先发条消息开始对话吧')
        return true
      }
      try {
        const r: any = await this.rpc('chat.compact', { session_id: sid })
        if (r?.ok) {
          await reply(r?.chars ? `🗜️ 上下文已压缩（${r.chars} 字符）` : '🗜️ 上下文已压缩')
        } else {
          await reply(`⚠️ 压缩未执行：${r?.message ?? '未知原因'}`)
        }
      } catch (e: any) {
        await reply(`❌ 压缩失败: ${e?.message ?? e}`)
      }
      return true
    }

    // /stop — abort the in-flight generation for this session
    if (trimmed === '/stop' || trimmed === '/停止') {
      const sid = this.getSid(config, conversationId)
      if (!sid) {
        await reply('⚠️ 当前没有活动会话')
        return true
      }
      try {
        const r: any = await this.rpc('chat.stop', { session_id: sid })
        await reply(r?.killed ? '⏹️ 已停止生成' : '（当前没有正在进行的生成）')
      } catch (e: any) {
        await reply(`❌ 停止失败: ${e?.message ?? e}`)
      }
      return true
    }

    // Unrecognized /xxx — fall through so the agent can see it (lets users
    // paste paths or code that start with '/' without being blocked).
    return false
  }
}
