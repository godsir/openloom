import { loomRpc } from './jsonrpc'
import { useStore } from '../stores'
import { streamBufferManager } from './stream-buffer'
import type { AttachedFile } from '../stores/input'
import type { QuotedSelection } from '../stores/selectionContext'
import { t } from '../i18n'

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export interface SendMessageOptions {
  sessionId: string
  content: string
  attachedFiles?: AttachedFile[]
  skills?: string[]
  /** For retry: skip appending a new user message (content is already in history). */
  skipUserMessage?: boolean
  /** Quoted selections captured via Ctrl+Shift+I inline editor. */
  quotedSelections?: QuotedSelection[]
  /** Override the global permission mode (e.g. write mode always uses 'operate'). */
  permissionMode?: string
}

/**
 * Check if the user's message is a scheduled-task request.
 * If detected, shows a confirmation dialog and creates the task.
 * Returns true if a cron task was created (caller should skip the AI send).
 *
 * Runs with a 4-second timeout: if the backend LLM detection takes too long,
 * we fall through to normal AI processing rather than blocking the UI.
 */
async function detectAndPromptCron(sessionId: string, content: string): Promise<boolean> {
  // Skip detection for very short messages
  if (!content || content.length < 4) return false

  try {
    // Race the detection RPC against a 4-second timeout.
    // If it takes longer than 4s, proceed with normal AI send.
    const result = await Promise.race([
      loomRpc<{
        should_create: boolean
        name?: string
        prompt?: string
        cron_expression?: string
        kind?: string
        confirmation?: string
      }>('cron.detect', { message: content }),
      new Promise<{ should_create: boolean }>((resolve) =>
        setTimeout(() => resolve({ should_create: false }), 4000)
      ),
    ])

    if (!result.should_create || !result.name) return false

    const confirmed = await useStore.getState().showCronDetected(
      result.name,
      result.prompt || '',
      result.cron_expression || '',
      result.kind || 'at',
      result.confirmation || '',
    )

    if (confirmed) {
      try {
        const createResult = await loomRpc<{ id: string }>('cron.create', {
          name: result.name,
          cron_expression: result.cron_expression,
          prompt: result.prompt,
          session_mode: 'isolated',
          timeout_secs: 300,
        })
        const msg = `Created scheduled task "${result.name}" (id: ${createResult.id})`
        useStore.getState().appendMessage(sessionId, {
          id: crypto.randomUUID(),
          role: 'assistant',
          blocks: [{ type: 'text', html: msg, source: msg }],
          timestamp: new Date().toISOString(),
        })
      } catch (e: any) {
        useStore.getState().addToast({ type: 'error', message: t('cron.createFailed', { error: e.message }) })
      }
      return true // Task created, skip AI send
    }
    // User cancelled -- fall through to normal AI send
  } catch (e: any) {
    console.warn('[cron] detection failed:', e?.message || e)
  }
  return false
}

/**
 * Send a message to the backend and manage the streaming state.
 * This is extracted from InputArea so it can be reused by resend/retry buttons.
 */
export async function sendMessage({ sessionId, content, attachedFiles = [], skills, skipUserMessage, quotedSelections = [], permissionMode }: SendMessageOptions): Promise<void> {
  const sid = sessionId
  useStore.getState().ensureSession(sid)

  // Append user message to chat immediately so the UI is responsive.
  // Detection runs in parallel — if a cron task is detected, we show the dialog
  // asynchronously without blocking the AI send.
  if (!skipUserMessage) {
    const msgId = crypto.randomUUID()
    const blocks: any[] = []

    // 1. Quoted selections first (context before instruction)
    for (const qs of quotedSelections) {
      blocks.push({
        type: 'quoted_selection',
        text: qs.text,
        filePath: qs.filePath,
        startLine: qs.startLine,
        endLine: qs.endLine,
      })
    }

    // 2. Text content
    if (content) {
      blocks.push({ type: 'text', html: escapeHtml(content).replace(/\n/g, '<br>'), source: content })
    }

    // 3. Attached files/images
    for (const f of attachedFiles) {
      if (f.mimeType.startsWith('image/')) {
        blocks.push({ type: 'image', path: f.path, name: f.name, mimeType: f.mimeType, thumbnail: f.thumbnail })
      } else {
        blocks.push({ type: 'file', path: f.path, name: f.name, mimeType: f.mimeType, size: f.size })
      }
    }

    useStore.getState().appendMessage(sid, {
      id: msgId, role: 'user',
      blocks,
      timestamp: new Date().toISOString(),
    })
  }

  // Fire cron detection in parallel — do not block the AI send.
  // If detection succeeds and user confirms, the task is created
  // independently of the AI conversation.
  detectAndPromptCron(sid, content)

  const aiMsgId = crypto.randomUUID()
  useStore.getState().addStreamingSession(sid)
  useStore.getState().appendMessage(sid, {
    id: aiMsgId, role: 'assistant',
    blocks: [],
    timestamp: new Date().toISOString(),
  })
  streamBufferManager.startStream(sid, aiMsgId, skills)

  const safetyTimer = setTimeout(() => {
    const buf = streamBufferManager.snapshot(sid)
    if (buf && buf.messageId === aiMsgId) {
      streamBufferManager.handleStreamEnd(sid)
    }
  }, 300_000) // 5 min safety timeout for long agent loops

  // Pet: user sends message → run right excitedly → wait for response
  import('./pet-sync').then(m => m.sendPetState('runRight'))
  setTimeout(() => import('./pet-sync').then(m => m.sendPetState('wait')), 400)

  try {
    const { currentModel, thinkingLevel } = useStore.getState()
    // Validate skill names before sending
    let validSkills = skills
    if (skills && skills.length > 0) {
      try {
        const res = await loomRpc<{ skills: { name: string }[] }>('skills.list')
        const known = new Set((res.skills ?? []).map(s => s.name))
        validSkills = skills.filter(s => known.has(s))
        const filtered = skills.filter(s => !known.has(s))
        if (filtered.length > 0) {
          useStore.getState().addToast({
            type: 'warning',
            message: `Unknown skill${filtered.length > 1 ? 's' : ''} ignored: ${filtered.join(', ')}`,
          })
        }
      } catch {
        // If skills.list fails, pass skills through as-is (backend will validate)
      }
    }

    const chatResult: any = await loomRpc('chat.send', {
      session_id: sid,
      content,
      model: currentModel || undefined,
      thinking_level: thinkingLevel || 'off',
      skills: validSkills && validSkills.length > 0 ? validSkills : undefined,
      skip_user_message: skipUserMessage || undefined,
      permission_mode: permissionMode || useStore.getState().permissionMode,
      attached_files: attachedFiles.map(f => ({
        path: f.path,
        name: f.name,
        size: f.size,
        mime_type: f.mimeType,
        thumbnail: f.thumbnail,
        content: f.content,
      })),
      quoted_selections: quotedSelections.map(q => ({
        text: q.text,
        file_path: q.filePath || '',
        start_line: q.startLine || 0,
        end_line: q.endLine || 0,
      })),
    })

    // Store stop_reason on assistant msg so ContinueButton can render on truncation
    if (chatResult && chatResult.stop_reason) {
      useStore.getState().setMessageStopReason(sid, aiMsgId, chatResult.stop_reason)
    }

    // Fallback: if chat.send didn't return stop_reason, query via session.last_stop_reason.
    // The backend populates stop_reason on the chat.send response; last_stop_reason exists
    // as a recovery path for WS reconnects where the chat.send response was lost.
    if (!chatResult || !chatResult.stop_reason) {
      try {
        const result: any = await loomRpc('session.last_stop_reason', { session_id: sid })
        if (result && result.stop_reason) {
          useStore.getState().setMessageStopReason(sid, aiMsgId, result.stop_reason)
        }
      } catch {
        // last_stop_reason is best-effort; ignore failures
      }
    }
  }
  catch (e: any) {
    useStore.getState().setInlineError(sid, e.message || t('sessions.sendFailed'))
  }
  finally {
    clearTimeout(safetyTimer)
    const buf = streamBufferManager.snapshot(sid)
    if (buf && buf.messageId === aiMsgId) {
      streamBufferManager.handleStreamEnd(sid)
    }
    // Update current session's modified time in sidebar
    const now = new Date().toISOString()
    useStore.getState().setSessions(
      useStore.getState().sessions.map(s =>
        s.path === sid ? { ...s, modified: now } : s
      )
    )
  }
}

/**
 * Send "继续" as a continuation — invisible user message (not shown in chat UI).
 * Used by the ContinueButton after agent_loop truncation (budget_exhausted / max_iterations).
 */
export async function sendContinuation(sessionId: string): Promise<void> {
  const store = useStore.getState()

  store.addStreamingSession(sessionId)

  const aiMsgId = crypto.randomUUID()
  store.appendMessage(sessionId, {
    id: aiMsgId, role: 'assistant',
    blocks: [],
    timestamp: new Date().toISOString(),
  })
  streamBufferManager.startStream(sessionId, aiMsgId)

  const safetyTimer = setTimeout(() => {
    const buf = streamBufferManager.snapshot(sessionId)
    if (buf && buf.messageId === aiMsgId) {
      streamBufferManager.handleStreamEnd(sessionId)
    }
  }, 300_000)

  try {
    const chatResult: any = await loomRpc('chat.send', {
      session_id: sessionId,
      content: '继续',
      skip_user_message: true,   // Don't persist "继续" in session history — invisible action
      model: store.currentModel || undefined,
      thinking_level: store.thinkingLevel || 'off',
      permission_mode: store.permissionMode,
    })

    // Store stop_reason in case the continuation also truncates
    if (chatResult && chatResult.stop_reason) {
      store.setMessageStopReason(sessionId, aiMsgId, chatResult.stop_reason)
    }
  } catch (e: any) {
    store.setInlineError(sessionId, e.message || t('sessions.sendFailed'))
  } finally {
    clearTimeout(safetyTimer)
    const buf = streamBufferManager.snapshot(sessionId)
    if (buf && buf.messageId === aiMsgId) {
      streamBufferManager.handleStreamEnd(sessionId)
    }
    const now = new Date().toISOString()
    store.setSessions(
      store.sessions.map(s =>
        s.path === sessionId ? { ...s, modified: now } : s
      )
    )
  }
}
