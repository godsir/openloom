import { useState, useEffect, useRef } from 'react'
import {
  useIMStore,
  statusKey,
  type InstanceConfig,
  type AccessMode,
} from '../../stores/im'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import Select, { type SelectOption } from '../shared/Select'
import ImWechatQrModal from './ImWechatQrModal'
import shared from '../shared/SettingsModal.module.css'
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

interface Props {
  config: InstanceConfig
}

export default function ImInstanceCard({ config }: Props) {
  const { t } = useLocale()
  const { saveConfig, deleteConfig, startChannel, stopChannel, statuses, sendHelp } = useIMStore()
  const agents = useStore((s: any) => s.agents) ?? []
  const addToast = useStore((s) => s.addToast)
  const dmPolicyOptions = useDmPolicyOptions()
  const groupPolicyOptions = useGroupPolicyOptions()

  const [expanded, setExpanded] = useState(false)
  const [showQrModal, setShowQrModal] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [nameDraft, setNameDraft] = useState(config.instanceName)
  const [allowFromDraft, setAllowFromDraft] = useState(config.allowFrom.join('\n'))
  const [groupAllowFromDraft, setGroupAllowFromDraft] = useState(config.groupAllowFrom.join('\n'))

  const debounceRef = useRef<number | null>(null)

  // Sync drafts when config changes externally.
  useEffect(() => { setNameDraft(config.instanceName) }, [config.instanceName])
  useEffect(() => { setAllowFromDraft(config.allowFrom.join('\n')) }, [config.allowFrom])
  useEffect(() => { setGroupAllowFromDraft(config.groupAllowFrom.join('\n')) }, [config.groupAllowFrom])

  const status = statuses[statusKey(config.platform, config.instanceId)]
  const connected = status?.connected ?? config.enabled
  const accountId = (config.configJson?.accountId as string) || status?.accountId || ''

  const agentOptions: SelectOption[] =
    agents.length > 0 ? agents.map((a: any) => ({ value: a.name, label: a.name })) : [{ value: 'main', label: 'main' }]

  // Debounced instance-name save (avoid IPC+DB write on every keystroke).
  useEffect(() => {
    if (nameDraft === config.instanceName) return
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = window.setTimeout(() => {
      saveConfig({ ...config, instanceName: nameDraft, updatedAt: Math.floor(Date.now() / 1000) })
    }, 500)
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current) }
  }, [nameDraft])

  const handleSendHelp = async () => {
    const res = await sendHelp(config.platform, config.instanceId)
    addToast({
      type: res.ok ? 'success' : 'error',
      message: res.ok
        ? t('im.helpSent', '帮助信息已发送')
        : `${t('im.helpSendFail', '发送失败')}: ${res.error ?? ''}`,
    })
  }

  const update = (patch: Partial<InstanceConfig>) => {
    saveConfig({ ...config, ...patch, updatedAt: Math.floor(Date.now() / 1000) })
  }

  const handleToggle = async (checked: boolean) => {
    if (checked) {
      const res = await startChannel(config.platform, config.instanceId)
      if (!res.ok) {
        addToast({ type: 'error', message: `${t('im.startFail', '启动失败')}${res.error ? `: ${res.error}` : ''}` })
        return
      }
      update({ enabled: true })
    } else {
      const res = await stopChannel(config.platform, config.instanceId)
      if (!res.ok) {
        addToast({ type: 'error', message: `${t('im.stopFail', '停止失败')}${res.error ? `: ${res.error}` : ''}` })
      }
      update({ enabled: false })
    }
  }

  const handleSave = async () => {
    await saveConfig({
      ...config,
      instanceName: nameDraft,
      allowFrom: allowFromDraft.split('\n').map((s) => s.trim()).filter(Boolean),
      groupAllowFrom: groupAllowFromDraft.split('\n').map((s) => s.trim()).filter(Boolean),
      updatedAt: Math.floor(Date.now() / 1000),
    })
    addToast({ type: 'success', message: t('im.saved', '已保存') })
  }

  const handleDelete = async () => {
    await deleteConfig(config.platform, config.instanceId)
    addToast({ type: 'success', message: t('im.deleted', '已删除') })
  }

  const fmtTime = (ts?: number | null): string => (ts ? new Date(ts).toLocaleTimeString() : '—')

  return (
    <div className={shared.mcpServerItem}>
      {/* Header */}
      <div className={shared.mcpServerHeader}>
        <div
          className={shared.mcpServerNameRow}
          onClick={() => setExpanded(!expanded)}
          style={{ cursor: 'pointer', flex: 1, minWidth: 0 }}
        >
          <span
            className={shared.mcpServerStatus}
            data-healthy={connected ? 'true' : 'false'}
            title={connected ? t('im.connected') : t('im.disconnected')}
          />
          <span className={shared.mcpServerName}>{config.instanceName}</span>
          {accountId && <span className={styles.accountId}>{accountId}</span>}
        </div>
        <div className={styles.instanceActions}>
          <button className={shared.mcpDisconnectBtn} onClick={handleSendHelp}>
            {t('im.connectTest')}
          </button>
          {config.platform === 'wechat' && !connected && (
            <button className={shared.mcpDisconnectBtn} onClick={() => setShowQrModal(true)}>
              {t('im.scanConnect')}
            </button>
          )}
          <div className={shared.mcpTransportToggle}>
            <button
              className={`${shared.mcpTransportBtn} ${connected ? shared.mcpTransportActive : ''}`}
              onClick={() => { if (!connected) handleToggle(true) }}
            >
              {t('im.start')}
            </button>
            <button
              className={`${shared.mcpTransportBtn} ${!connected ? shared.mcpTransportActive : ''}`}
              onClick={() => { if (connected) handleToggle(false) }}
            >
              {t('im.stop')}
            </button>
          </div>
        </div>
      </div>

      {/* Runtime stats */}
      <div className={styles.instanceMeta}>
        <span>{t('im.lastInbound')}: {fmtTime(status?.lastInboundAt)}</span>
        <span>{t('im.lastOutbound')}: {fmtTime(status?.lastOutboundAt)}</span>
        {status?.lastError && <span style={{ color: 'var(--red)' }}>{status.lastError}</span>}
      </div>

      {/* Expanded config */}
      {expanded && (
        <div className={shared.aboutSection} style={{ marginTop: 10, paddingTop: 10, borderTop: '1px solid var(--border)' }}>
          <div className={shared.themeLabel}>{t('im.basicInfo')}</div>
          <div className={shared.mcpFormRow}>
            <label className={shared.mcpFormLabel}>{t('im.instanceName')}</label>
            <input
              className={shared.mcpFormInput}
              value={nameDraft}
              onChange={(e) => setNameDraft(e.target.value)}
            />
          </div>
          <div className={shared.mcpFormRow}>
            <label className={shared.mcpFormLabel}>{t('im.instanceId', 'Instance ID')}</label>
            <input className={shared.mcpFormInput} value={config.instanceId} disabled />
          </div>

          <div className={shared.themeLabel} style={{ marginTop: 8 }}>{t('im.dmPolicy')}</div>
          <Select
            value={config.dmPolicy}
            options={dmPolicyOptions}
            onChange={(v) => update({ dmPolicy: v as AccessMode })}
            variant="form"
          />

          <div className={shared.themeLabel} style={{ marginTop: 8 }}>{t('im.groupPolicy')}</div>
          <Select
            value={config.groupPolicy}
            options={groupPolicyOptions}
            onChange={(v) => update({ groupPolicy: v as InstanceConfig['groupPolicy'] })}
            variant="form"
          />

          <div className={shared.mcpFormRow} style={{ marginTop: 8 }}>
            <label className={shared.mcpFormLabel}>{t('im.allowFrom', '私聊白名单')}</label>
            <textarea
              className={shared.mcpFormInput}
              style={{ height: 'auto', minHeight: 60, resize: 'vertical' }}
              value={allowFromDraft}
              onChange={(e) => setAllowFromDraft(e.target.value)}
              placeholder={t('im.allowFromHint', '每行一个 ID，留空表示禁用')}
            />
          </div>
          <div className={shared.mcpFormRow}>
            <label className={shared.mcpFormLabel}>{t('im.groupAllowFrom', '群聊白名单')}</label>
            <textarea
              className={shared.mcpFormInput}
              style={{ height: 'auto', minHeight: 60, resize: 'vertical' }}
              value={groupAllowFromDraft}
              onChange={(e) => setGroupAllowFromDraft(e.target.value)}
              placeholder={t('im.allowFromHint', '每行一个 ID，留空表示禁用')}
            />
          </div>

          <div className={shared.mcpFormRow}>
            <label className={shared.mcpFormLabel}>{t('im.agentId', '绑定 Agent')}</label>
            <Select
              value={config.agentId || 'main'}
              options={agentOptions}
              onChange={(v) => update({ agentId: v })}
              variant="form"
            />
          </div>

          {config.platform === 'wechat' && (
            <p className={shared.toolsEmpty} style={{ margin: 0 }}>
              {t('im.wechatConfigHint', '微信通过扫码连接，无需手动填写凭证')}
            </p>
          )}

          {/* Actions */}
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 8, alignItems: 'center' }}>
            {confirmDelete ? (
              <>
                <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{t('im.deleteConfirm', '确定删除？')}</span>
                <button className={shared.mcpCancelBtn} onClick={() => setConfirmDelete(false)}>
                  {t('common.cancel')}
                </button>
                <button
                  className={shared.mcpDisconnectBtn}
                  style={{ color: 'var(--red)', borderColor: 'var(--red)' }}
                  onClick={handleDelete}
                >
                  {t('common.delete')}
                </button>
              </>
            ) : (
              <>
                <button className={shared.mcpDisconnectBtn} onClick={() => setConfirmDelete(true)}>
                  {t('common.delete')}
                </button>
                <button className={shared.mcpConnectBtn} onClick={handleSave}>
                  {t('im.saveConfig')}
                </button>
              </>
            )}
          </div>
        </div>
      )}

      {showQrModal && (
        <ImWechatQrModal
          instanceId={config.instanceId}
          instanceName={config.instanceName}
          onClose={() => setShowQrModal(false)}
          onConnected={() => {}}
        />
      )}
    </div>
  )
}
