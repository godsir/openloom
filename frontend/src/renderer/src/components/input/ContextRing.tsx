import { useStore } from '../../stores'
import styles from './ContextRing.module.css'

const DEFAULT_MAX_TOKENS = 200_000

export default function ContextRing() {
  const sessionId = useStore((s) => s.currentSessionId)
  const messages = useStore((s) => sessionId ? s.messagesBySession.get(sessionId) : undefined)
  const liveUsage = useStore((s) => sessionId ? s.usageBySession.get(sessionId) : undefined)
  const models = useStore((s) => s.models)
  const currentModel = useStore((s) => s.currentModel)

  // Prefer the live usage broadcast for the current turn. Fall back to the
  // usage stamped onto the last assistant message — that's what makes ring
  // state survive session close→reopen and full reload (history rehydrate).
  let usage = liveUsage
  if (!usage && messages) {
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i]
      if (m.role === 'assistant' && m.usage) {
        usage = {
          prompt: m.usage.prompt,
          completion: m.usage.completion,
          model: m.usage.model ?? '',
          contextWindow: m.usage.contextWindow ?? 0,
        }
        break
      }
    }
  }

  if (!usage) return null
  const { prompt, completion } = usage
  const total = prompt + completion
  if (total === 0) return null

  // Scale: backend-reported window for the model that produced this usage,
  // else the current session model's configured context_size, else 200k.
  // Mid-session model switch: next turn's chat.token_usage event arrives with
  // the new model/window, so the ring rescales automatically.
  const scale =
    (usage.contextWindow && usage.contextWindow > 0 ? usage.contextWindow : 0) ||
    models.find((m) => m.name === (usage.model || currentModel))?.context_size ||
    DEFAULT_MAX_TOKENS

  const pct = Math.min((total / scale) * 100, 100)
  const circ = 2 * Math.PI * 7
  const offset = circ * (1 - pct / 100)
  const color = pct > 80 ? 'var(--red)' : pct > 50 ? 'var(--amber)' : 'var(--accent)'

  const fmt = (n: number) => n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
  const fmtScale = (n: number) => n >= 1000 ? `${Math.round(n / 1000)}k` : String(n)

  return (
    <div className={styles.wrapper}>
      <svg width="18" height="18" className={styles.ring}>
        <circle cx="9" cy="9" r="7" fill="none" stroke="rgba(34,211,238,0.12)" strokeWidth="2" />
        <circle cx="9" cy="9" r="7" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round"
          strokeDasharray={circ} strokeDashoffset={offset}
          style={{ transition: 'stroke-dashoffset 0.5s ease' }} />
      </svg>
      <span className={styles.centerLabel}>
        {total >= 1000 ? `${(total / 1000).toFixed(0)}k` : total}
      </span>

      <div className={styles.tooltip}>
        <div className={styles.tooltipRow}>
          <span>上下文用量</span>
          <span className={styles.tooltipVal}>{pct.toFixed(1)}%</span>
        </div>
        <div className={styles.tooltipRow}>
          <span>输入 tokens</span>
          <span className={styles.tooltipVal}>{fmt(prompt)}</span>
        </div>
        <div className={styles.tooltipRow}>
          <span>输出 tokens</span>
          <span className={styles.tooltipVal}>{fmt(completion)}</span>
        </div>
        <hr className={styles.tooltipDivider} />
        <div className={styles.tooltipRow}>
          <span>总计</span>
          <span className={styles.tooltipVal}>{fmt(total)} / {fmtScale(scale)}</span>
        </div>
        {usage.model && (
          <div className={styles.tooltipRow}>
            <span>模型</span>
            <span className={styles.tooltipVal}>{usage.model}</span>
          </div>
        )}
      </div>
    </div>
  )
}
