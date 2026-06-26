import { useState, useEffect, useRef } from 'react'
import { useIMStore } from '../../stores/im'
import styles from './ImTab.module.css'

interface Props {
  instanceId: string
  instanceName: string
  onClose: () => void
  onConnected: (accountId: string) => void
}

export default function ImWechatQrModal({ instanceId, instanceName, onClose, onConnected }: Props) {
  const { wechatQrStart, wechatQrWait } = useIMStore()
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null)
  const [sessionKey, setSessionKey] = useState<string | null>(null)
  const [status, setStatus] = useState<'loading' | 'waiting' | 'connected' | 'expired' | 'error'>('loading')
  const [message, setMessage] = useState('')
  const timerRef = useRef<number | null>(null)
  const mountedRef = useRef(true)

  useEffect(() => {
    mountedRef.current = true
    startLogin()
    return () => {
      mountedRef.current = false
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [])

  const startLogin = async () => {
    try {
      setStatus('loading')
      const result = await wechatQrStart(instanceId)
      if (!mountedRef.current) return
      setQrDataUrl(result.qrDataUrl)
      setSessionKey(result.sessionKey)
      setStatus('waiting')
      // Auto-poll
      pollForScan(result.sessionKey)
    } catch (err: any) {
      if (mountedRef.current) {
        setStatus('error')
        setMessage(err.message || 'Failed to start login')
      }
    }
  }

  const pollForScan = async (key: string) => {
    // Poll every 2 seconds
    const poll = async () => {
      if (!mountedRef.current) return
      try {
        const result = await wechatQrWait(instanceId, key)
        if (!mountedRef.current) return
        if (result.connected) {
          setStatus('connected')
          if (result.accountId) {
            onConnected(result.accountId)
          }
        } else if (result.message?.includes('expired')) {
          setStatus('expired')
          setMessage('二维码已过期')
        } else {
          // Keep polling
          timerRef.current = window.setTimeout(poll, 2000)
        }
      } catch {
        timerRef.current = window.setTimeout(poll, 2000)
      }
    }
    timerRef.current = window.setTimeout(poll, 2000)
  }

  return (
    <div className={styles.modalOverlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <div className={styles.modalHeader}>
          <h3>微信扫码连接 — {instanceName}</h3>
          <button className={styles.closeBtn} onClick={onClose}>✕</button>
        </div>
        <div className={styles.modalBody} style={{ textAlign: 'center' }}>
          {status === 'loading' && <p className={styles.loading}>正在生成二维码...</p>}
          {status === 'error' && <p className={styles.errorText}>{message || '启动失败'}</p>}
          {(status === 'waiting' || status === 'expired') && (
            <>
              {qrDataUrl ? (
                <img src={qrDataUrl} alt="WeChat QR Code" className={styles.qrImage} />
              ) : (
                <div className={styles.qrPlaceholder}>
                  <p>二维码加载中...</p>
                  <p className={styles.qrHint}>请用手机微信扫描二维码</p>
                </div>
              )}
              <p className={styles.qrStatus}>
                {status === 'waiting' ? '🟢 等待扫码中...' : '⏰ 二维码已过期'}
              </p>
              <p className={styles.qrHint}>有效期：5 分钟</p>
              {status === 'expired' && (
                <button className={styles.refreshBtn} onClick={startLogin}>↻ 刷新二维码</button>
              )}
            </>
          )}
          {status === 'connected' && (
            <div className={styles.connectedMsg}>
              <span className={styles.connectedIcon}>✓</span>
              <p>连接成功！</p>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
