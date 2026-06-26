import { useState, useEffect, useRef } from 'react'
import { useIMStore } from '../../stores/im'
import { useLocale } from '../../i18n'
import { IconCheck, IconClock, IconRefresh, IconX, IconQrCode } from '../../utils/icons'
import { QRCodeSVG } from 'qrcode.react'
import styles from './ImTab.module.css'

interface Props {
  instanceId: string
  instanceName: string
  onClose: () => void
  onConnected: (accountId: string) => void
}

const QR_TTL_SECONDS = 300

export default function ImWechatQrModal({ instanceId, instanceName, onClose, onConnected }: Props) {
  const { t } = useLocale()
  const { wechatQrStart, wechatQrWait } = useIMStore()
  const [qrContent, setQrContent] = useState<string | null>(null)
  const [sessionKey, setSessionKey] = useState<string | null>(null)
  const [status, setStatus] = useState<'loading' | 'waiting' | 'connected' | 'expired' | 'error'>('loading')
  const [message, setMessage] = useState('')
  const [remaining, setRemaining] = useState(QR_TTL_SECONDS)
  const timerRef = useRef<number | null>(null)
  const countdownRef = useRef<number | null>(null)
  const mountedRef = useRef(true)

  useEffect(() => {
    mountedRef.current = true
    startLogin()
    return () => {
      mountedRef.current = false
      if (timerRef.current) clearTimeout(timerRef.current)
      if (countdownRef.current) clearInterval(countdownRef.current)
    }
  }, [])

  const startCountdown = () => {
    if (countdownRef.current) clearInterval(countdownRef.current)
    setRemaining(QR_TTL_SECONDS)
    countdownRef.current = window.setInterval(() => {
      setRemaining((prev) => {
        if (prev <= 1) {
          if (countdownRef.current) clearInterval(countdownRef.current)
          if (mountedRef.current) setStatus('expired')
          return 0
        }
        return prev - 1
      })
    }, 1000)
  }

  const startLogin = async () => {
    if (countdownRef.current) clearInterval(countdownRef.current)
    try {
      setStatus('loading')
      const result = await wechatQrStart(instanceId)
      if (!mountedRef.current) return
      setQrContent(result.qrContent)
      setSessionKey(result.sessionKey)
      setStatus('waiting')
      startCountdown()
      pollForScan(result.sessionKey)
    } catch (err: any) {
      if (mountedRef.current) {
        setStatus('error')
        setMessage(err.message || t('im.qrStartFail'))
      }
    }
  }

  const pollForScan = async (key: string) => {
    const poll = async () => {
      if (!mountedRef.current) return
      try {
        const result = await wechatQrWait(instanceId, key)
        if (!mountedRef.current) return
        if (result.connected) {
          setStatus('connected')
          if (countdownRef.current) clearInterval(countdownRef.current)
          if (result.accountId) onConnected(result.accountId)
        } else if (result.message?.includes('expired') || result.message?.includes('过期') || result.message?.includes('過期')) {
          setStatus('expired')
          if (countdownRef.current) clearInterval(countdownRef.current)
        } else {
          timerRef.current = window.setTimeout(poll, 2000)
        }
      } catch {
        timerRef.current = window.setTimeout(poll, 2000)
      }
    }
    timerRef.current = window.setTimeout(poll, 2000)
  }

  const mm = Math.floor(remaining / 60)
  const ss = String(remaining % 60).padStart(2, '0')

  return (
    <div className={styles.modalOverlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <div className={styles.modalHeader}>
          <h3>{t('im.qrTitle')} — {instanceName}</h3>
          <button className={styles.closeBtn} onClick={onClose}><IconX size={16} /></button>
        </div>

        <div className={styles.modalBodyCentered}>
          {/* Loading */}
          {status === 'loading' && (
            <p className={styles.loading}>{t('im.qrLoading')}</p>
          )}

          {/* Error */}
          {status === 'error' && (
            <p className={styles.errorText}>{message || t('im.qrStartFail')}</p>
          )}

          {/* QR waiting / expired */}
          {(status === 'waiting' || status === 'expired') && (
            <>
              {qrContent ? (
                <div className={styles.qrCard}>
                  <QRCodeSVG value={qrContent} size={208} bgColor="#ffffff" fgColor="#000000" level="M" />
                </div>
              ) : (
                <div className={styles.qrPlaceholder}>
                  <IconQrCode size={32} style={{ opacity: 0.3 }} />
                  <p>{t('im.qrLoadingHint')}</p>
                </div>
              )}

              <div className={`${styles.qrBadge} ${status === 'waiting' ? styles.qrBadgeScanning : styles.qrBadgeExpired}`}>
                <IconClock size={12} />
                {status === 'waiting' ? t('im.qrWaiting') : t('im.qrExpired')}
              </div>

              {status === 'waiting' && (
                <>
                  <div className={styles.qrCountdown}>{mm}:{ss}</div>
                  <span className={styles.qrHintRow} style={{ marginTop: 2 }}>
                    {t('im.qrValidMinutes')} · {t('im.countdown')}
                  </span>
                </>
              )}

              <p className={styles.qrHintRow} style={{ marginTop: 8 }}>
                {t('im.qrHint')}
              </p>

              {status === 'expired' && (
                <button className={styles.refreshBtn} onClick={startLogin}>
                  <IconRefresh size={12} />
                  {t('im.qrRefresh')}
                </button>
              )}
            </>
          )}

          {/* Connected */}
          {status === 'connected' && (
            <div style={{ textAlign: 'center', padding: '20px 0' }}>
              <div className={styles.qrConnectedIcon}>
                <IconCheck size={32} />
              </div>
              <p className={styles.qrConnectedTitle}>{t('im.qrConnected')}</p>
              <p className={styles.qrConnectedSub}>{instanceName}</p>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
