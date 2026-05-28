import { useStore } from '../../stores'
import styles from './ContextRing.module.css'

const MAX_TOKENS = 200000

export default function ContextRing() {
  const { prompt, completion } = useStore((s) => s.tokenUsage)
  const total = prompt + completion
  if (total === 0) return null

  const pct = Math.min((total / MAX_TOKENS) * 100, 100)
  const circ = 2 * Math.PI * 7
  const offset = circ * (1 - pct / 100)
  const color = pct > 80 ? 'var(--red)' : pct > 50 ? 'var(--amber)' : 'var(--accent)'

  const fmt = (n: number) => n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)

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
          <span className={styles.tooltipVal}>{fmt(total)}</span>
        </div>
      </div>
    </div>
  )
}
