interface Props {
  messages: Array<{ status: string; statusMessage?: string | null }>
  streamingSessions: Set<string>
  currentSessionId: string | null
  inlineErrors: Map<string, { text: string }>
}

export default function InputStatusBars({
  messages,
  streamingSessions,
  currentSessionId,
  inlineErrors,
}: Props) {
  const isStreaming =
    !!currentSessionId && streamingSessions.has(currentSessionId)
  const error = currentSessionId
    ? inlineErrors.get(currentSessionId)?.text
    : null

  // Check last agent state for subagent activity
  const lastAgentMsg = [...messages].reverse().find((m) => m.status === 'subagent')

  return (
    <div className="space-y-1">
      {isStreaming && (
        <div className="flex items-center gap-2 px-3 py-1 text-xs text-blue-400 bg-blue-900/20 rounded-md">
          <span className="animate-pulse">●</span>
          <span>AI 正在回复...</span>
        </div>
      )}
      {lastAgentMsg && (
        <div className="flex items-center gap-2 px-3 py-1 text-xs text-purple-400 bg-purple-900/20 rounded-md">
          <span>子 Agent 运行中: {lastAgentMsg.statusMessage || '处理中'}</span>
        </div>
      )}
      {error && (
        <div className="flex items-center gap-2 px-3 py-1 text-xs text-red-400 bg-red-900/20 rounded-md">
          <span>!</span>
          <span>{error}</span>
        </div>
      )}
    </div>
  )
}
