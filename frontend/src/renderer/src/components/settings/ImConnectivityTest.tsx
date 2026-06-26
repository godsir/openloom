import { useState, useEffect } from 'react'
import { useIMStore, type Platform, type ConnectivityResult } from '../../stores/im'
import styles from './ImTab.module.css'

interface Props {
  platform: Platform
  instanceId: string
  onClose: () => void
}

export default function ImConnectivityTest({ platform, instanceId, onClose }: Props) {
  const { testConnectivity } = useIMStore()
  const [result, setResult] = useState<ConnectivityResult | null>(null)
  const [running, setRunning] = useState(false)

  const runTest = async () => {
    setRunning(true)
    try {
      const res = await testConnectivity(platform, instanceId)
      setResult(res)
    } finally {
      setRunning(false)
    }
  }

  // Auto-run on mount
  useEffect(() => { runTest() }, [])

  const levelIcon = (level: string) => {
    switch (level) {
      case 'pass': return '✓'
      case 'warn': return '⚠'
      case 'fail': return '✕'
      default: return 'ℹ'
    }
  }

  return (
    <div className={styles.modalOverlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <div className={styles.modalHeader}>
          <h3>连接测试</h3>
          <button className={styles.closeBtn} onClick={onClose}>✕</button>
        </div>
        <div className={styles.modalBody}>
          {running && <p className={styles.loading}>正在测试...</p>}
          {result && (
            <>
              <div className={`${styles.verdictBanner} ${result.verdict === 'pass' ? styles.verdictPass : result.verdict === 'warn' ? styles.verdictWarn : styles.verdictFail}`}>
                <span className={styles.verdictIcon}>
                  {result.verdict === 'pass' ? '✓' : result.verdict === 'warn' ? '⚠' : '✕'}
                </span>
                <div>
                  <div className={styles.verdictText}>
                    {result.verdict === 'pass' ? '连接正常' : result.verdict === 'warn' ? '需要关注' : '连接失败'}
                  </div>
                  <div className={styles.verdictTime}>{new Date(result.testedAt).toLocaleString()}</div>
                </div>
              </div>
              <div className={styles.checkList}>
                {result.checks.map((check, i) => (
                  <div key={i} className={`${styles.checkItem} ${styles[`check${check.level.charAt(0).toUpperCase() + check.level.slice(1)}`]}`}>
                    <span className={styles.checkIcon}>{levelIcon(check.level)}</span>
                    <div className={styles.checkContent}>
                      <div className={styles.checkMessage}>{check.message}</div>
                      {check.suggestion && <div className={styles.checkSuggestion}>{check.suggestion}</div>}
                    </div>
                    <span className={`${styles.checkBadge} ${styles[`badge${check.level.charAt(0).toUpperCase() + check.level.slice(1)}`]}`}>
                      {check.level}
                    </span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
        <div className={styles.modalFooter}>
          <button className={styles.closeBtn} onClick={onClose}>关闭</button>
        </div>
      </div>
    </div>
  )
}
