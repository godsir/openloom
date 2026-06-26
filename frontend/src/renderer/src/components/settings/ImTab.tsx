import { useEffect } from 'react'
import {
  useIMStore,
  PLATFORM_LABELS,
  PLATFORM_ORDER,
  IMPLEMENTED_PLATFORMS,
  type InstanceConfig,
  type AccessMode,
} from '../../stores/im'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import Select, { type SelectOption } from '../shared/Select'
import ImInstanceCard from './ImInstanceCard'
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

function generateId(): string {
  return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`
}

export default function ImTab() {
  const { t } = useLocale()
  const {
    instances, settings, selectedPlatform, loading,
    loadConfigs, loadSettings, saveSettings, saveConfig,
    setSelectedPlatform, subscribeEvents, stopChannel,
  } = useIMStore()
  const agents = useStore((s: any) => s.agents) ?? []
  const addToast = useStore((s) => s.addToast)
  const dmPolicyOptions = useDmPolicyOptions()

  useEffect(() => {
    loadConfigs()
    loadSettings()
    const unsub = subscribeEvents()
    return unsub
  }, [])

  const platformInstances = instances.filter((i) => i.platform === selectedPlatform)
  const isImplemented = IMPLEMENTED_PLATFORMS.includes(selectedPlatform)

  const agentOptions: SelectOption[] =
    agents.length > 0
      ? agents.map((a: any) => ({ value: a.name, label: a.name }))
      : [{ value: 'main', label: 'main' }]

  const handleAddInstance = async () => {
    const newConfig: InstanceConfig = {
      id: generateId(),
      platform: selectedPlatform,
      instanceId: `instance_${Date.now()}`,
      instanceName: `${PLATFORM_LABELS[selectedPlatform]} ${platformInstances.length + 1}`,
      enabled: false,
      configJson: {},
      dmPolicy: settings.defaultDmPolicy,
      allowFrom: [],
      groupPolicy: 'disabled',
      groupAllowFrom: [],
      agentId: settings.defaultAgentId,
      createdAt: Math.floor(Date.now() / 1000),
      updatedAt: Math.floor(Date.now() / 1000),
    }
    await saveConfig(newConfig)
    addToast({ type: 'success', message: t('im.instanceAdded', '已添加实例') })
  }

  const handleToggleGlobal = async (checked: boolean) => {
    await saveSettings({ globalEnabled: checked })
    if (!checked) {
      // Stop all running channels when the global switch turns off.
      const running = instances.filter((i) => i.enabled)
      await Promise.all(running.map((i) => stopChannel(i.platform, i.instanceId)))
    }
    addToast({
      type: 'success',
      message: checked
        ? t('im.globalOnToast', '已开启 IM')
        : t('im.globalOffToast', '已关闭 IM，所有通道已停止'),
    })
  }

  return (
    <>
      {/* Global settings */}
      <div className={shared.aboutSection}>
        <div className={shared.themeLabel}>{t('im.globalSettings')}</div>

        <div className={shared.aboutRow}>
          <div>
            <span className={shared.aboutLabel}>{t('im.globalEnable')}</span>
            <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>
              {t('im.globalEnableHint', '全局 IM 总开关，关闭后停止所有通道')}
            </p>
          </div>
          <div className={shared.mcpTransportToggle}>
            <button
              className={`${shared.mcpTransportBtn} ${settings.globalEnabled ? shared.mcpTransportActive : ''}`}
              onClick={() => { if (!settings.globalEnabled) handleToggleGlobal(true) }}
            >
              {t('im.enable')}
            </button>
            <button
              className={`${shared.mcpTransportBtn} ${!settings.globalEnabled ? shared.mcpTransportActive : ''}`}
              onClick={() => { if (settings.globalEnabled) handleToggleGlobal(false) }}
            >
              {t('im.disable')}
            </button>
          </div>
        </div>

        <div className={shared.aboutRow}>
          <div>
            <span className={shared.aboutLabel}>{t('im.defaultDmPolicy')}</span>
            <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('im.defaultDmPolicyHint')}</p>
          </div>
          <div style={{ width: 180 }}>
            <Select
              value={settings.defaultDmPolicy}
              options={dmPolicyOptions}
              onChange={(v) => saveSettings({ defaultDmPolicy: v as AccessMode })}
              variant="form"
            />
          </div>
        </div>

        <div className={shared.aboutRow}>
          <div>
            <span className={shared.aboutLabel}>{t('im.skillsEnabled')}</span>
            <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>
              {t('im.skillsEnabledHint', '允许 Agent 在 IM 会话中调用 Skills')}
            </p>
          </div>
          <div className={shared.mcpTransportToggle}>
            <button
              className={`${shared.mcpTransportBtn} ${settings.skillsEnabled ? shared.mcpTransportActive : ''}`}
              onClick={() => { if (!settings.skillsEnabled) saveSettings({ skillsEnabled: true }) }}
            >
              {t('im.enable')}
            </button>
            <button
              className={`${shared.mcpTransportBtn} ${!settings.skillsEnabled ? shared.mcpTransportActive : ''}`}
              onClick={() => { if (settings.skillsEnabled) saveSettings({ skillsEnabled: false }) }}
            >
              {t('im.disable')}
            </button>
          </div>
        </div>

        <div className={shared.aboutRow}>
          <div>
            <span className={shared.aboutLabel}>{t('im.bindAgent')}</span>
            <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>
              {t('im.bindAgentHint', '新实例默认绑定的 Agent')}
            </p>
          </div>
          <div style={{ width: 180 }}>
            <Select
              value={settings.defaultAgentId}
              options={agentOptions}
              onChange={(v) => saveSettings({ defaultAgentId: v })}
              variant="form"
            />
          </div>
        </div>
      </div>

      <hr className={shared.sectionDivider} />

      {/* Platform picker */}
      <div className={styles.platformTabs}>
        {PLATFORM_ORDER.map((p) => (
          <button
            key={p}
            className={`${styles.platformTab} ${selectedPlatform === p ? styles.platformTabActive : ''}`}
            onClick={() => setSelectedPlatform(p)}
          >
            {PLATFORM_LABELS[p]}
          </button>
        ))}
      </div>

      {/* Instances */}
      {!isImplemented ? (
        <p className={shared.toolsEmpty}>
          {t('im.notImplemented', `${PLATFORM_LABELS[selectedPlatform]} 接入尚未实现，暂仅支持微信`)}
        </p>
      ) : loading ? (
        <p className={shared.toolsEmpty}>{t('common.loading')}</p>
      ) : platformInstances.length === 0 ? (
        <p className={shared.toolsEmpty}>{t('im.noInstances')}</p>
      ) : (
        <div className={shared.mcpServerList}>
          {platformInstances.map((inst) => (
            <ImInstanceCard key={inst.id} config={inst} />
          ))}
        </div>
      )}

      {isImplemented && (
        <button className={shared.mcpAddBtn} onClick={handleAddInstance}>
          + {t('im.addInstance')}
        </button>
      )}
    </>
  )
}
