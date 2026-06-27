import { useState, useEffect, useRef } from 'react'
import {
  useIMStore,
  PLATFORM_LABELS,
  statusKey,
  type InstanceConfig,
  type AccessMode,
} from '../../stores/im'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import Select, { type SelectOption } from '../shared/Select'
import PlatformIcon from '../shared/PlatformIcon'
import ImWechatQrModal from './ImWechatQrModal'
import ImConnectivityTest from './ImConnectivityTest'
import styles from './ImTab.module.css'

function useDmPolicyOptions(): SelectOption[] {
  const { t } = useLocale()
  return [
    { value: 'pairing', label: t('im.pairing') },
    { value: 'allowlist', label: t('im.allowlist') },
    { value: 'open', label: t('im.open') },
    { value: 'disabled', label: t('im.disabled') },
  ]
}

function useGroupPolicyOptions(): SelectOption[] {
  const { t } = useLocale()
  return [
    { value: 'allowlist', label: t('im.allowlist') },
    { value: 'open', label: t('im.open') },
    { value: 'disabled', label: t('im.disabled') },
  ]
}

function fmtTime(ts?: number | null): string {
  return ts ? new Date(ts).toLocaleTimeString() : '—'
}

interface Props { config: InstanceConfig }

export default function ImInstanceCard({ config }: Props) {
  const { t } = useLocale()
  const { saveConfig, deleteConfig, startChannel, stopChannel, statuses, sendHelp, telegramLogin } = useIMStore()
  const agents = useStore((s: any) => s.agents) ?? []
  const addToast = useStore((s) => s.addToast)
  const dmPolicyOptions = useDmPolicyOptions()
  const groupPolicyOptions = useGroupPolicyOptions()

  const [expanded, setExpanded] = useState(false)
  const [showQr, setShowQr] = useState(false)
  const [showTest, setShowTest] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [nameDraft, setNameDraft] = useState(config.instanceName)
  const [allowDraft, setAllowDraft] = useState(config.allowFrom.join('\n'))
  const [groupAllowDraft, setGroupAllowDraft] = useState(config.groupAllowFrom.join('\n'))
  const [tokenDraft, setTokenDraft] = useState('');
  const [loginLoading, setLoginLoading] = useState(false);

  const debounceRef = useRef<number | null>(null)

  useEffect(() => { setNameDraft(config.instanceName) }, [config.instanceName])
  useEffect(() => { setAllowDraft(config.allowFrom.join('\n')) }, [config.allowFrom])
  useEffect(() => { setGroupAllowDraft(config.groupAllowFrom.join('\n')) }, [config.groupAllowFrom])

  const status = statuses[statusKey(config.platform, config.instanceId)]
  const connected = status?.connected ?? config.enabled
  const accountId = (config.configJson?.accountId as string) || status?.accountId || ''

  const agentOptions: SelectOption[] = agents.length > 0
    ? agents.map((a: any) => ({ value: a.name, label: a.name }))
    : [{ value: 'main', label: 'main' }]

  useEffect(() => {
    if (nameDraft === config.instanceName) return
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = window.setTimeout(() => {
      saveConfig({ ...config, instanceName: nameDraft, updatedAt: Math.floor(Date.now() / 1000) })
    }, 500)
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current) }
  }, [nameDraft])

  const update = (patch: Partial<InstanceConfig>) => {
    saveConfig({ ...config, ...patch, updatedAt: Math.floor(Date.now() / 1000) })
  }

  const handleToggle = async (checked: boolean) => {
    if (checked) {
      const res = await startChannel(config.platform, config.instanceId)
      if (!res.ok) {
        addToast({ type: 'error', message: `${t('im.startFail')}${res.error ? `: ${res.error}` : ''}` })
        return
      }
      update({ enabled: true })
    } else {
      const res = await stopChannel(config.platform, config.instanceId)
      if (!res.ok) addToast({ type: 'error', message: `${t('im.stopFail')}${res.error ? `: ${res.error}` : ''}` })
      update({ enabled: false })
    }
  }

  const handleSave = async () => {
    await saveConfig({
      ...config,
      instanceName: nameDraft,
      allowFrom: allowDraft.split('\n').map(s => s.trim()).filter(Boolean),
      groupAllowFrom: groupAllowDraft.split('\n').map(s => s.trim()).filter(Boolean),
      updatedAt: Math.floor(Date.now() / 1000),
    })
    addToast({ type: 'success', message: t('im.saved') })
  }

  const handleDelete = async () => {
    await deleteConfig(config.platform, config.instanceId)
    addToast({ type: 'success', message: t('im.deleted') })
  }

  const handleSendHelp = async () => {
    const res = await sendHelp(config.platform, config.instanceId)
    addToast({
      type: res.ok ? 'success' : 'error',
      message: res.ok ? t('im.helpSent') : `${t('im.helpSendFail')}: ${res.error ?? ''}`,
    })
  }

  const handleTelegramLogin = async () => {
    const trimmed = tokenDraft.trim();
    if (!trimmed) {
      addToast({ type: 'error', message: t('im.telegramTokenEmpty', '请输入 Bot Token') });
      return;
    }
    setLoginLoading(true);
    try {
      const res = await telegramLogin(config.platform, config.instanceId, trimmed);
      if (res.ok) {
        addToast({ type: 'success', message: t('im.telegramConnected', '已连接到 Telegram') });
        setTokenDraft('');
      } else {
        addToast({ type: 'error', message: `${t('im.telegramLoginFail', '连接失败')}: ${res.error ?? ''}` });
      }
    } finally {
      setLoginLoading(false);
    }
  };

  return (
    <>
      <div className={styles.instanceCard}>
        {/* ── Top row: identity + actions ── */}
        <div className={styles.instanceCardTop}>
          <div className={styles.instanceIdentity}>
            <span className={`${styles.instanceDot} ${connected ? styles.instanceDotLive : styles.instanceDotOff}`} />
            <PlatformIcon platform={config.platform} size={18} />
            <div className={styles.instanceText}>
              <span className={styles.instanceName}>{config.instanceName}</span>
              {accountId && <span className={styles.instanceAccount}>{accountId}</span>}
            </div>
          </div>
          <div className={styles.instanceActions}>
            {/* connect test */}
            <button className={styles.instanceBtn} onClick={() => setShowTest(true)} title={t('im.connectTest')}>
              {t('im.connectTest')}
            </button>
            {/* QR button — wechat only, when disconnected */}
            {config.platform === 'wechat' && !connected && (
              <button className={styles.instanceBtn} onClick={() => setShowQr(true)}>
                {t('im.scanConnect')}
              </button>
            )}
            {/* Telegram Token 输入 — 未连接时显示 */}
            {config.platform === 'telegram' && !connected && (
              <>
                <input
                  className={styles.configInput}
                  style={{ width: 180, height: 26, fontSize: 10 }}
                  type="password"
                  value={tokenDraft}
                  onChange={(e) => setTokenDraft(e.target.value)}
                  placeholder={t('im.telegramToken', 'Bot Token')}
                  title={t('im.telegramTokenHint', '在 @BotFather 创建 Bot 获取 Token')}
                />
                <button
                  className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`}
                  onClick={handleTelegramLogin}
                  disabled={loginLoading}
                >
                  {loginLoading ? '...' : t('im.telegramLogin', '连接')}
                </button>
              </>
            )}
            {/* start / stop */}
            <button
              className={`${styles.instanceBtn} ${connected ? styles.instanceBtnDanger : styles.instanceBtnPrimary}`}
              onClick={() => handleToggle(!connected)}
            >
              {connected ? t('im.stop') : t('im.start')}
            </button>
          </div>
        </div>

        {/* ── Stats row ── */}
        <div className={styles.instanceStats}>
          <span className={styles.statItem}>
            <span className={`${styles.statDot} ${styles.statDotIn}`} />
            {t('im.lastInbound')}: {fmtTime(status?.lastInboundAt)}
          </span>
          <span className={styles.statItem}>
            <span className={`${styles.statDot} ${styles.statDotOut}`} />
            {t('im.lastOutbound')}: {fmtTime(status?.lastOutboundAt)}
          </span>
          {status?.lastError && (
            <span className={styles.statError}>{status.lastError}</span>
          )}
        </div>

        {/* ── Expanded config ── */}
        {expanded && (
          <div className={styles.configPanel}>
            <div className={styles.configGrid}>
              <div className={styles.configField}>
                <label className={styles.configLabel}>{t('im.instanceName')}</label>
                <input className={styles.configInput} value={nameDraft} onChange={(e) => setNameDraft(e.target.value)} />
              </div>
              <div className={styles.configField}>
                <label className={styles.configLabel}>{t('im.bindAgent')}</label>
                <Select value={config.agentId || 'main'} options={agentOptions} onChange={(v) => update({ agentId: v })} variant="form" />
              </div>
              <div className={styles.configField}>
                <label className={styles.configLabel}>{t('im.dmPolicy')}</label>
                <Select value={config.dmPolicy} options={dmPolicyOptions} onChange={(v) => update({ dmPolicy: v as AccessMode })} variant="form" />
              </div>
              <div className={styles.configField}>
                <label className={styles.configLabel}>{t('im.groupPolicy')}</label>
                <Select value={config.groupPolicy} options={groupPolicyOptions} onChange={(v) => update({ groupPolicy: v as InstanceConfig['groupPolicy'] })} variant="form" />
              </div>
              <div className={`${styles.configField} ${styles.configFull}`}>
                <label className={styles.configLabel}>{t('im.allowFrom')}</label>
                <textarea className={styles.configTextarea} value={allowDraft} onChange={(e) => setAllowDraft(e.target.value)} placeholder={t('im.allowFromHint')} />
              </div>
              <div className={`${styles.configField} ${styles.configFull}`}>
                <label className={styles.configLabel}>{t('im.groupAllowFrom')}</label>
                <textarea className={styles.configTextarea} value={groupAllowDraft} onChange={(e) => setGroupAllowDraft(e.target.value)} placeholder={t('im.allowFromHint')} />
              </div>
            </div>

            {config.platform === 'wechat' && (
              <p className={styles.empty} style={{ padding: 0, textAlign: 'left', fontSize: 10 }}>{t('im.wechatConfigHint')}</p>
            )}
            {config.platform === 'telegram' && (
              <p className={styles.empty} style={{ padding: 0, textAlign: 'left', fontSize: 10 }}>{t('im.telegramConfigHint', '在 @BotFather 使用 /newbot 创建 Bot，获取 Token 后粘贴到上方输入框')}</p>
            )}

            <div className={styles.configActions}>
              {confirmDelete ? (
                <>
                  <button className={styles.instanceBtn} onClick={() => setConfirmDelete(false)}>{t('common.cancel')}</button>
                  <button className={`${styles.instanceBtn} ${styles.instanceBtnDanger}`} onClick={handleDelete}>{t('common.delete')}</button>
                </>
              ) : (
                <>
                  <button className={styles.instanceBtn} onClick={() => setConfirmDelete(true)}>{t('common.delete')}</button>
                  <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handleSave}>{t('im.saveConfig')}</button>
                </>
              )}
            </div>
          </div>
        )}
      </div>

      {/* expand toggle (invisible click area on stats row) */}
      <div
        onClick={() => setExpanded(!expanded)}
        style={{ cursor: 'pointer', fontSize: 10, color: 'var(--text-muted)', textAlign: 'center', padding: '2px 0 0', userSelect: 'none' }}
      >
        {expanded ? '收起配置 ▲' : '展开配置 ▼'}
      </div>

      {showQr && (
        <ImWechatQrModal
          instanceId={config.instanceId}
          instanceName={config.instanceName}
          onClose={() => setShowQr(false)}
          onConnected={() => {}}
        />
      )}
      {showTest && (
        <ImConnectivityTest
          platform={config.platform}
          instanceId={config.instanceId}
          onClose={() => setShowTest(false)}
        />
      )}
    </>
  )
}
