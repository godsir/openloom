import { useStore } from '../../stores'

export default function ContextRing() {
  const tokenUsage = useStore((s) => s.tokenUsage)
  const { prompt, completion } = tokenUsage
  const total = prompt + completion

  // Default 200k context window estimate
  const maxTokens = 200000
  const pct = Math.min((total / maxTokens) * 100, 100)
  const circumference = 2 * Math.PI * 9
  const dashOffset = circumference * (1 - pct / 100)

  if (total === 0) return null

  return (
    <div className="relative group shrink-0" title={`${total.toLocaleString()} / ${(maxTokens / 1000).toFixed(0)}k tokens`}>
      <svg width="22" height="22" className="-rotate-90">
        <circle cx="11" cy="11" r="9" fill="none" stroke="rgb(39,39,42)" strokeWidth="2" />
        <circle
          cx="11" cy="11" r="9" fill="none"
          stroke={pct > 80 ? 'rgb(239,68,68)' : pct > 50 ? 'rgb(234,179,8)' : 'rgb(59,130,246)'}
          strokeWidth="2" strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={dashOffset}
          className="transition-[stroke-dashoffset] duration-500"
        />
      </svg>
      <div className="absolute inset-0 flex items-center justify-center">
        <span className="text-[8px] text-zinc-400 font-mono">
          {total >= 1000 ? `${(total / 1000).toFixed(0)}k` : total}
        </span>
      </div>
      {/* Tooltip */}
      <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 px-2 py-1 bg-zinc-800 text-xs text-zinc-300 rounded whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none">
        Prompt: {prompt.toLocaleString()} | Completion: {completion.toLocaleString()}
      </div>
    </div>
  )
}
