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
import ImPopoQrModal from './ImPopoQrModal'
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
  const { saveConfig, deleteConfig, startChannel, stopChannel, statuses, sendHelp, telegramLogin, discordLogin, qqLogin, feishuLogin, wecomLogin, dingtalkLogin, popoLogin } = useIMStore()
  const agents = useStore((s: any) => s.agents) ?? []
  const addToast = useStore((s) => s.addToast)
  const dmPolicyOptions = useDmPolicyOptions()
  const groupPolicyOptions = useGroupPolicyOptions()

  const [expanded, setExpanded] = useState(false)
  const [showQr, setShowQr] = useState(false)
  const [showPopoQr, setShowPopoQr] = useState(false)
  const [showTest, setShowTest] = useState(false)
  const [nameDraft, setNameDraft] = useState(config.instanceName)
  const [allowDraft, setAllowDraft] = useState(config.allowFrom.join('\n'))
  const [groupAllowDraft, setGroupAllowDraft] = useState(config.groupAllowFrom.join('\n'))
  const [tokenDraft, setTokenDraft] = useState('');
  const [loginLoading, setLoginLoading] = useState(false);
  const [discordToken, setDiscordToken] = useState('');
  const [qqAppId, setQqAppId] = useState(''); const [qqSecret, setQqSecret] = useState('');
  const [feishuAppId, setFeishuAppId] = useState(''); const [feishuSecret, setFeishuSecret] = useState('');
  const [wecomCorpId, setWecomCorpId] = useState(''); const [wecomSecret, setWecomSecret] = useState(''); const [wecomAgentId, setWecomAgentId] = useState('');
  const [dtAppKey, setDtAppKey] = useState(''); const [dtSecret, setDtSecret] = useState('');
  const [popoAppKey, setPopoAppKey] = useState(''); const [popoAppSecret, setPopoAppSecret] = useState(''); const [popoAesKey, setPopoAesKey] = useState('');

  const debounceRef = useRef<number | null>(null)
  const configRef = useRef(config)
  configRef.current = config

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
      saveConfig({ ...configRef.current, instanceName: nameDraft, updatedAt: Math.floor(Date.now() / 1000) })
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
    // Merge tokenDraft into configJson so "Save" persists credentials too.
    // This prevents the bug where user enters token but clicks Save instead of Connect.
    const updatedConfigJson = { ...config.configJson } as Record<string, unknown>
    if (tokenDraft && tokenDraft.trim()) {
      updatedConfigJson.token = tokenDraft.trim()
    }
    await saveConfig({
      ...config,
      instanceName: nameDraft,
      configJson: updatedConfigJson,
      allowFrom: allowDraft.split('\n').map(s => s.trim()).filter(Boolean),
      groupAllowFrom: groupAllowDraft.split('\n').map(s => s.trim()).filter(Boolean),
      updatedAt: Math.floor(Date.now() / 1000),
    })
    addToast({ type: 'success', message: t('im.saved') })
  }

  const handleDelete = async () => {
    const ok = await useStore.getState().showConfirm(
      t('im.deleteConfirmTitle'),
      t('im.deleteConfirmMessage', { name: config.instanceName }),
      true,
    )
    if (!ok) return
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

  const handleDiscordLogin = async () => {
    const trimmed = discordToken.trim();
    if (!trimmed) {
      addToast({ type: 'error', message: t('im.discordTokenEmpty', '请输入 Bot Token') });
      return;
    }
    setLoginLoading(true);
    try {
      const res = await discordLogin(config.platform, config.instanceId, trimmed);
      if (res.ok) {
        addToast({ type: 'success', message: t('im.discordConnected', '已连接到 Discord') });
        setDiscordToken('');
      } else {
        addToast({ type: 'error', message: `${t('im.discordLoginFail', '连接失败')}: ${res.error ?? ''}` });
      }
    } finally {
      setLoginLoading(false);
    }
  };

  const handleQqLogin = async () => {
    const id = qqAppId.trim(); const sec = qqSecret.trim();
    if (!id || !sec) {
      addToast({ type: 'error', message: t('im.qqCredEmpty', '请输入 App ID 和 Client Secret') });
      return;
    }
    setLoginLoading(true);
    try {
      const res = await qqLogin(config.platform, config.instanceId, id, sec);
      if (res.ok) {
        addToast({ type: 'success', message: t('im.qqConnected', '已连接到 QQ') });
        setQqAppId(''); setQqSecret('');
      } else {
        addToast({ type: 'error', message: `${t('im.qqLoginFail', '连接失败')}: ${res.error ?? ''}` });
      }
    } finally {
      setLoginLoading(false);
    }
  };

  const handleFeishuLogin = async () => {
    const id = feishuAppId.trim(); const sec = feishuSecret.trim();
    if (!id || !sec) {
      addToast({ type: 'error', message: t('im.feishuCredEmpty', '请输入 App ID 和 App Secret') });
      return;
    }
    setLoginLoading(true);
    try {
      const res = await feishuLogin(config.platform, config.instanceId, id, sec);
      if (res.ok) {
        addToast({ type: 'success', message: t('im.feishuConnected', '已连接到飞书') });
        setFeishuAppId(''); setFeishuSecret('');
      } else {
        addToast({ type: 'error', message: `${t('im.feishuLoginFail', '连接失败')}: ${res.error ?? ''}` });
      }
    } finally {
      setLoginLoading(false);
    }
  };

  const handleWecomLogin = async () => {
    const cid = wecomCorpId.trim(); const sec = wecomSecret.trim(); const aid = wecomAgentId.trim();
    if (!cid || !sec || !aid) {
      addToast({ type: 'error', message: t('im.wecomCredEmpty', '请填写所有必填字段') });
      return;
    }
    setLoginLoading(true);
    try {
      const res = await wecomLogin(config.platform, config.instanceId, cid, sec, aid);
      if (res.ok) {
        addToast({ type: 'success', message: t('im.wecomConnected', '已连接到企业微信') });
        setWecomCorpId(''); setWecomSecret(''); setWecomAgentId('');
      } else {
        addToast({ type: 'error', message: `${t('im.wecomLoginFail', '连接失败')}: ${res.error ?? ''}` });
      }
    } finally {
      setLoginLoading(false);
    }
  };

  const handleDingtalkLogin = async () => {
    const ak = dtAppKey.trim(); const sec = dtSecret.trim();
    if (!ak || !sec) {
      addToast({ type: 'error', message: t('im.dingtalkCredEmpty', '请输入 App Key 和 App Secret') });
      return;
    }
    setLoginLoading(true);
    try {
      const res = await dingtalkLogin(config.platform, config.instanceId, ak, sec);
      if (res.ok) {
        addToast({ type: 'success', message: t('im.dingtalkConnected', '已连接到钉钉') });
        setDtAppKey(''); setDtSecret('');
      } else {
        addToast({ type: 'error', message: `${t('im.dingtalkLoginFail', '连接失败')}: ${res.error ?? ''}` });
      }
    } finally {
      setLoginLoading(false);
    }
  };

  const handlePopoLogin = async () => {
    const ak = popoAppKey.trim(); const sec = popoAppSecret.trim(); const aes = popoAesKey.trim();
    if (!ak || !sec || !aes) {
      addToast({ type: 'error', message: t('im.popoCredEmpty', '请填写所有凭据字段') });
      return;
    }
    setLoginLoading(true);
    try {
      const res = await popoLogin(config.platform, config.instanceId, ak, sec, aes);
      if (res.ok) {
        addToast({ type: 'success', message: t('im.popoConnected', '已连接到 POPO') });
        setPopoAppKey(''); setPopoAppSecret(''); setPopoAesKey('');
      } else {
        addToast({ type: 'error', message: `${t('im.popoLoginFail', '连接失败')}: ${res.error ?? ''}` });
      }
    } finally {
      setLoginLoading(false);
    }
  };

  return (
    <>
      <div className={styles.instanceCard} onClick={() => setExpanded(!expanded)} style={{ cursor: 'pointer' }}>
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
          <div className={styles.instanceActions} onClick={(e) => e.stopPropagation()}>
            {/* connect test */}
            <button className={styles.instanceBtn} onClick={() => setShowTest(true)} title={t('im.connectTest')}>
              {t('im.connectTest')}
            </button>
            {/* credentials + QR moved into the expanded config panel */}
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
          <span
            className={styles.expandHint}
            onClick={(e) => { e.stopPropagation(); setExpanded(!expanded) }}
          >
            {expanded ? '收起配置 ▲' : '展开配置 ▼'}
          </span>
        </div>

        {/* ── Expanded config ── */}
        {expanded && (
          <div className={styles.configPanel} onClick={(e) => e.stopPropagation()}>
            {/* 连接凭据 — 未连接时显示在配置区顶部 */}
            {!connected && (
              <div className={styles.credSection}>
                <div className={styles.credSectionLabel}>{t('im.connection', '连接凭据')}</div>

                {config.platform === 'wechat' && (
                  <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={() => setShowQr(true)}>
                    {t('im.scanConnect')}
                  </button>
                )}

                {config.platform === 'telegram' && (
                  <div className={styles.credRow}>
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="password" value={tokenDraft} onChange={(e) => setTokenDraft(e.target.value)} placeholder={t('im.telegramToken', 'Bot Token')} title={t('im.telegramTokenHint', '在 @BotFather 创建 Bot 获取 Token')} />
                    <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handleTelegramLogin} disabled={loginLoading}>{loginLoading ? '...' : t('im.telegramLogin', '连接')}</button>
                  </div>
                )}

                {config.platform === 'discord' && (
                  <div className={styles.credRow}>
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="password" value={discordToken} onChange={(e) => setDiscordToken(e.target.value)} placeholder={t('im.discordToken', 'Bot Token')} title={t('im.discordTokenHint', '在 Discord Developer Portal 创建 Bot')} />
                    <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handleDiscordLogin} disabled={loginLoading}>{loginLoading ? '...' : t('im.discordLogin', '连接')}</button>
                  </div>
                )}

                {config.platform === 'qq' && (
                  <div className={styles.credRow}>
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="text" value={qqAppId} onChange={(e) => setQqAppId(e.target.value)} placeholder={t('im.qqAppId', 'App ID')} />
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="password" value={qqSecret} onChange={(e) => setQqSecret(e.target.value)} placeholder={t('im.qqClientSecret', 'Client Secret')} />
                    <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handleQqLogin} disabled={loginLoading}>{loginLoading ? '...' : t('im.qqLogin', '连接')}</button>
                  </div>
                )}

                {config.platform === 'feishu' && (
                  <div className={styles.credRow}>
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="text" value={feishuAppId} onChange={(e) => setFeishuAppId(e.target.value)} placeholder={t('im.feishuAppId', 'App ID')} />
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="password" value={feishuSecret} onChange={(e) => setFeishuSecret(e.target.value)} placeholder={t('im.feishuAppSecret', 'App Secret')} />
                    <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handleFeishuLogin} disabled={loginLoading}>{loginLoading ? '...' : t('im.feishuLogin', '连接')}</button>
                  </div>
                )}

                {config.platform === 'wecom' && (
                  <div className={styles.credRow}>
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="text" value={wecomCorpId} onChange={(e) => setWecomCorpId(e.target.value)} placeholder={t('im.wecomCorpId', 'Corp ID')} />
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="password" value={wecomSecret} onChange={(e) => setWecomSecret(e.target.value)} placeholder={t('im.wecomSecret', 'Secret')} />
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="text" value={wecomAgentId} onChange={(e) => setWecomAgentId(e.target.value)} placeholder={t('im.wecomAgentId', 'Agent ID')} />
                    <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handleWecomLogin} disabled={loginLoading}>{loginLoading ? '...' : t('im.wecomLogin', '连接')}</button>
                  </div>
                )}

                {config.platform === 'dingtalk' && (
                  <div className={styles.credRow}>
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="text" value={dtAppKey} onChange={(e) => setDtAppKey(e.target.value)} placeholder={t('im.dingtalkAppKey', 'App Key')} />
                    <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="password" value={dtSecret} onChange={(e) => setDtSecret(e.target.value)} placeholder={t('im.dingtalkAppSecret', 'App Secret')} />
                    <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handleDingtalkLogin} disabled={loginLoading}>{loginLoading ? '...' : t('im.dingtalkLogin', '连接')}</button>
                  </div>
                )}

                {config.platform === 'popo' && (
                  <>
                    <div className={styles.credRow}>
                      <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="text" value={popoAppKey} onChange={(e) => setPopoAppKey(e.target.value)} placeholder="App Key" />
                      <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="password" value={popoAppSecret} onChange={(e) => setPopoAppSecret(e.target.value)} placeholder="App Secret" />
                      <input className={styles.configInput} style={{ flex: 1, minWidth: 0 }} type="password" value={popoAesKey} onChange={(e) => setPopoAesKey(e.target.value)} placeholder="AES Key" />
                      <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handlePopoLogin} disabled={loginLoading}>{loginLoading ? '...' : t('im.popoLogin', '连接')}</button>
                    </div>
                    <div className={styles.credRow}>
                      <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>{t('im.or', '或')}</span>
                      <button className={styles.instanceBtn} onClick={() => setShowPopoQr(true)}>{t('im.popoQr', 'QR 扫码连接')}</button>
                    </div>
                  </>
                )}
              </div>
            )}

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
            {config.platform === 'discord' && (
              <p className={styles.empty} style={{ padding: 0, textAlign: 'left', fontSize: 10 }}>{t('im.discordConfigHint', '在 Discord Developer Portal 创建 Bot，获取 Token 后粘贴到上方输入框')}</p>
            )}
            {config.platform === 'qq' && (
              <p className={styles.empty} style={{ padding: 0, textAlign: 'left', fontSize: 10 }}>{t('im.qqConfigHint', '在 QQ 开放平台创建应用，获取 App ID 和 Client Secret 后填写到上方输入框')}</p>
            )}
            {config.platform === 'feishu' && (
              <p className={styles.empty} style={{ padding: 0, textAlign: 'left', fontSize: 10 }}>{t('im.feishuConfigHint', '在飞书开放平台创建应用，获取 App ID 和 App Secret 后填写到上方输入框')}</p>
            )}
            {config.platform === 'wecom' && (
              <p className={styles.empty} style={{ padding: 0, textAlign: 'left', fontSize: 10 }}>{t('im.wecomConfigHint', '在企业微信管理后台获取 Corp ID、Secret 和 Agent ID，填写到上方输入框')}</p>
            )}
            {config.platform === 'dingtalk' && (
              <p className={styles.empty} style={{ padding: 0, textAlign: 'left', fontSize: 10 }}>{t('im.dingtalkConfigHint', '在钉钉开放平台创建应用，获取 App Key 和 App Secret 后填写到上方输入框')}</p>
            )}
            {config.platform === 'popo' && (
              <p className={styles.empty} style={{ padding: 0, textAlign: 'left', fontSize: 10 }}>{t('im.popoConfigHint', '填入 App Key / App Secret / AES Key 后点连接，或点击 QR 扫码授权')}</p>
            )}

            <div className={styles.configActions}>
              <button className={`${styles.instanceBtn} ${styles.instanceBtnDanger}`} onClick={handleDelete}>{t('common.delete')}</button>
              <button className={`${styles.instanceBtn} ${styles.instanceBtnPrimary}`} onClick={handleSave}>{t('im.saveConfig')}</button>
            </div>
          </div>
        )}
      </div>

      {showQr && config.platform === 'wechat' && (
        <ImWechatQrModal
          instanceId={config.instanceId}
          instanceName={config.instanceName}
          onClose={() => setShowQr(false)}
          onConnected={() => {}}
        />
      )}
      {showPopoQr && config.platform === 'popo' && (
        <ImPopoQrModal
          instanceId={config.instanceId}
          instanceName={config.instanceName}
          onClose={() => setShowPopoQr(false)}
          onConnected={async () => { await startChannel(config.platform, config.instanceId); }}
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
