import { useStore } from '../../stores'

const WS_STATE_LABELS: Record<string, string> = {
  connected: '已连接',
  reconnecting: '重连中...',
  disconnected: '未连接',
}

export default function StatusBar() {
  const wsState = useStore((s) => s.wsState)
  const currentModel = useStore((s) => s.currentModel)
  const tokenUsage = useStore((s) => s.tokenUsage)

  return (
    <div className="flex items-center justify-between px-3 py-1 text-xs text-zinc-600 bg-zinc-950 border-t border-zinc-800 h-6 shrink-0">
      <div className="flex items-center gap-2">
        <span
          className={`inline-block w-2 h-2 rounded-full ${
            wsState === 'connected'
              ? 'bg-green-500'
              : wsState === 'reconnecting'
                ? 'bg-yellow-500 animate-pulse'
                : 'bg-red-500'
          }`}
        />
        <span>{WS_STATE_LABELS[wsState]}</span>
      </div>
      <div className="flex items-center gap-3">
        {currentModel && <span>{currentModel}</span>}
        {tokenUsage.prompt > 0 && (
          <span>
            Tokens: {tokenUsage.prompt + tokenUsage.completion}
          </span>
        )}
      </div>
    </div>
  )
}
