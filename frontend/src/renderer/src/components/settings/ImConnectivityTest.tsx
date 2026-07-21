import { useState, useEffect } from 'react'
import { useIMStore, type Platform, type ConnectivityResult } from '../../stores/im'
import { useLocale } from '../../i18n'
import { IconCheck, IconAlertCircle, IconXCircle, IconInfo, IconX } from '../../utils/icons'
import styles from './ImTab.module.css'

interface Props {
  platform: Platform
  instanceId: string
  onClose: () => void
}

export default function ImConnectivityTest({ platform, instanceId, onClose }: Props) {
  const { t } = useLocale()
  const { testConnectivity } = useIMStore()
  const [result, setResult] = useState<ConnectivityResult | null>(null)
  const [running, setRunning] = useState(false)
  const [testError, setTestError] = useState<string | null>(null)

  const runTest = async () => {
    setRunning(true)
    setTestError(null)
    try {
      const res = await testConnectivity(platform, instanceId)
      setResult(res)
    } catch (e: any) {
      // 检测本身失败（RPC 异常等）时给出可见反馈与重试，而非空白弹窗（A9）
      setTestError(e?.message || String(e))
    } finally {
      setRunning(false)
    }
  }

  useEffect(() => { runTest() }, [])

  const levelIcon = (level: string) => {
    switch (level) {
      case 'pass': return <IconCheck size={14} />
      case 'warn': return <IconAlertCircle size={14} />
      case 'fail': return <IconXCircle size={14} />
      default: return <IconInfo size={14} />
    }
  }

  const verdictLabel = (v: string) =>
    v === 'pass' ? t('im.verdictPass')
    : v === 'warn' ? t('im.verdictWarn')
    : t('im.verdictFail')

  const verdictIcon = (v: string) => {
    switch (v) {
      case 'pass': return <IconCheck size={28} />
      case 'warn': return <IconAlertCircle size={28} />
      case 'fail': return <IconXCircle size={28} />
      default: return <IconInfo size={28} />
    }
  }

  const bigIconClass = result
    ? result.verdict === 'pass' ? styles.verdictBigPass
      : result.verdict === 'warn' ? styles.verdictBigWarn
      : styles.verdictBigFail
    : undefined

  return (
    <div className={styles.modalOverlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <div className={styles.modalHeader}>
          <h3>{t('im.connectTestTitle')}</h3>
          <button className={styles.closeBtn} onClick={onClose}><IconX size={16} /></button>
        </div>
        <div className={styles.modalBody}>
          {running && <p className={styles.loading}>{t('im.testRunning')}</p>}

          {!running && testError && (
            <div className={styles.testErrorBox}>
              <p className={styles.statError}>{t('im.testFailed')}: {testError}</p>
              <button className={styles.instanceBtn} onClick={runTest}>
                {t('common.retry')}
              </button>
            </div>
          )}

          {result && (
            <>
              {/* Big verdict */}
              {bigIconClass && (
                <div className={`${styles.verdictBigIcon} ${bigIconClass}`}>
                  {verdictIcon(result.verdict)}
                </div>
              )}
              <p className={styles.verdictBigTitle}>{verdictLabel(result.verdict)}</p>
              <p className={styles.verdictBigTime}>{new Date(result.testedAt).toLocaleString()}</p>

              {/* Check list */}
              <div className={styles.checkList}>
                {result.checks.map((check, i) => {
                  const cls = check.level === 'pass' ? styles.checkPass
                    : check.level === 'warn' ? styles.checkWarn
                    : check.level === 'fail' ? styles.checkFail
                    : styles.checkInfo
                  const badgeCls = check.level === 'pass' ? styles.badgePass
                    : check.level === 'warn' ? styles.badgeWarn
                    : check.level === 'fail' ? styles.badgeFail
                    : styles.badgeInfo
                  return (
                    <div key={i} className={`${styles.checkItem} ${cls}`}>
                      <span className={styles.checkIcon}>{levelIcon(check.level)}</span>
                      <div className={styles.checkContent}>
                        <div className={styles.checkMessage}>{check.message}</div>
                        {check.suggestion && <div className={styles.checkSuggestion}>{check.suggestion}</div>}
                      </div>
                      <span className={`${styles.checkBadge} ${badgeCls}`}>{check.level}</span>
                    </div>
                  )
                })}
              </div>
            </>
          )}
        </div>
        <div className={styles.modalFooter}>
          <button className={styles.instanceBtn} onClick={onClose}>{t('common.close')}</button>
        </div>
      </div>
    </div>
  )
}
