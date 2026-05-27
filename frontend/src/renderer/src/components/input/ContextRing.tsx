import { useStore } from '../../stores'

export default function ContextRing() {
  const { prompt, completion } = useStore((s) => s.tokenUsage)
  const total = prompt + completion
  if (total === 0) return null

  const maxTokens = 200000
  const pct = Math.min((total / maxTokens) * 100, 100)
  const circ = 2 * Math.PI * 7
  const offset = circ * (1 - pct / 100)
  const color = pct > 80 ? 'var(--red)' : pct > 50 ? 'var(--amber)' : 'var(--accent)'

  return (
    <div className="relative group shrink-0" title={`${total.toLocaleString()} tokens`}>
      <svg width="18" height="18" className="-rotate-90">
        <circle cx="9" cy="9" r="7" fill="none" stroke="rgba(0,227,199,0.1)" strokeWidth="2" />
        <circle cx="9" cy="9" r="7" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round"
          strokeDasharray={circ} strokeDashoffset={offset}
          className="transition-[stroke-dashoffset] duration-500" />
      </svg>
      <span className="absolute inset-0 flex items-center justify-center text-[7px] text-[var(--text-muted)] font-medium">
        {total >= 1000 ? `${(total / 1000).toFixed(0)}k` : total}
      </span>
    </div>
  )
}
