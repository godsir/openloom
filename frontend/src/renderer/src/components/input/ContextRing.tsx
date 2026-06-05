import { useStore } from '../../stores'
import styles from './ContextRing.module.css'

const DEFAULT_MAX_TOKENS = 200_000

function fmt(n: number): string {
  return n >= 1000000 ? `${(n / 1000000).toFixed(1)}M` : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

function fmtScale(n: number): string {
  return n >= 1000000 ? `${Math.round(n / 1000000)}M` : n >= 1000 ? `${Math.round(n / 1000)}k` : String(n)
}

function fmtCost(n: number): string {
  if (n <= 0) return '¥0'
  if (n < 0.001) return '<¥0.001'
  return '¥' + n.toFixed(4).replace(/0+$/, '').replace(/\.$/, '')
}

function calcTurnCost(
  prompt: number,
  completion: number,
  cacheRead: number,
  cacheWrite: number,
  inputPrice: number,
  outputPrice: number,
  cacheReadPrice: number,
  cacheWritePrice: number,
): number {
  const promptNonCache = Math.max(prompt - cacheRead, 0)
  return (
    promptNonCache * inputPrice +
    cacheRead * cacheReadPrice +
    cacheWrite * cacheWritePrice +
    completion * outputPrice
  ) / 1_000_000
}

export default function ContextRing() {
  const sessionId = useStore((s) => s.currentSessionId)
  const messages = useStore((s) => sessionId ? s.messagesBySession.get(sessionId) : undefined)
  const liveUsage = useStore((s) => sessionId ? s.usageBySession.get(sessionId) : undefined)
  const sessionCum = useStore((s) => sessionId ? s.sessionCumulative.get(sessionId) : undefined)
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
          cached: m.usage.cached ?? 0,
          cacheRead: m.usage.cacheRead ?? 0,
          cacheWrite: m.usage.cacheWrite ?? 0,
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
  const scale =
    (usage.contextWindow && usage.contextWindow > 0 ? usage.contextWindow : 0) ||
    models.find((m) => m.name === (usage.model || currentModel))?.context_size ||
    DEFAULT_MAX_TOKENS

  const pct = Math.min((total / scale) * 100, 100)
  const circ = 2 * Math.PI * 7
  const offset = circ * (1 - pct / 100)
  const color = pct > 80 ? 'var(--red)' : pct > 50 ? 'var(--amber)' : 'var(--accent)'

  // Find model pricing
  const modelInfo = models.find((m) => m.name === (usage.model || currentModel))
  const inputPrice = modelInfo?.input_price || 0
  const outputPrice = modelInfo?.output_price || 0
  const cacheReadPrice = modelInfo?.cache_read_price || 0
  const cacheWritePrice = modelInfo?.cache_write_price || 0

  const cacheRead = usage.cacheRead || 0
  const cacheWrite = usage.cacheWrite || 0
  const promptNonCache = Math.max(prompt - cacheRead, 0)

  const turnCost = calcTurnCost(prompt, completion, cacheRead, cacheWrite, inputPrice, outputPrice, cacheReadPrice, cacheWritePrice)
  const hasPrice = inputPrice > 0 || outputPrice > 0

  return (
    <div className={styles.wrapper}>
      <svg width="18" height="18" className={styles.ring}>
        <circle cx="9" cy="9" r="7" fill="none" stroke="rgba(34,211,238,0.12)" strokeWidth="2" />
        <circle cx="9" cy="9" r="7" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round"
          strokeDasharray={circ} strokeDashoffset={offset}
          className={styles.ringProgress} />
      </svg>
      <span className={styles.centerLabel}>
        {total >= 1000000 ? `${(total / 1000000).toFixed(1)}M` : total >= 1000 ? `${(total / 1000).toFixed(0)}k` : total}
      </span>

      <div className={styles.tooltip}>
        <div className={styles.tooltipRow}>
          <span>上下文用量</span>
          <span className={styles.tooltipVal}>{pct.toFixed(1)}%</span>
        </div>
        <div className={styles.tooltipRow}>
          <span>输入 (未命中)</span>
          <span className={styles.tooltipVal}>{fmt(promptNonCache)}</span>
        </div>
        {cacheRead > 0 && (
          <div className={styles.tooltipRow}>
            <span>输入 (缓存命中)</span>
            <span className={styles.tooltipVal}>{fmt(cacheRead)}</span>
          </div>
        )}
        {cacheWrite > 0 && (
          <div className={styles.tooltipRow}>
            <span>缓存写入</span>
            <span className={styles.tooltipVal}>{fmt(cacheWrite)}</span>
          </div>
        )}
        <div className={styles.tooltipRow}>
          <span>输出 tokens</span>
          <span className={styles.tooltipVal}>{fmt(completion)}</span>
        </div>
        <hr className={styles.tooltipDivider} />
        <div className={styles.tooltipRow}>
          <span>总计</span>
          <span className={styles.tooltipVal}>{fmt(total)} / {fmtScale(scale)}</span>
        </div>
        {hasPrice && (
          <div className={styles.tooltipRow}>
            <span>本次消耗</span>
            <span className={styles.tooltipVal}>{fmtCost(turnCost)}</span>
          </div>
        )}
        {sessionCum && sessionCum.requests > 0 && (
          <>
            <hr className={styles.tooltipDivider} />
            <div className={styles.tooltipRow}>
              <span>会话累计请求</span>
              <span className={styles.tooltipVal}>{sessionCum.requests} 次</span>
            </div>
            <div className={styles.tooltipRow}>
              <span>会话累计输入</span>
              <span className={styles.tooltipVal}>{fmt(sessionCum.prompt)}</span>
            </div>
            <div className={styles.tooltipRow}>
              <span>会话累计输出</span>
              <span className={styles.tooltipVal}>{fmt(sessionCum.completion)}</span>
            </div>
            {sessionCum.cacheRead > 0 && (
              <div className={styles.tooltipRow}>
                <span>会话累计缓存命中</span>
                <span className={styles.tooltipVal}>{fmt(sessionCum.cacheRead)}</span>
              </div>
            )}
            <div className={styles.tooltipRow}>
              <span>会话累计消耗</span>
              <span className={styles.tooltipVal}>{fmtCost(sessionCum.cost)}</span>
            </div>
          </>
        )}
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
