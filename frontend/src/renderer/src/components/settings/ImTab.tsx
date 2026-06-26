import { useState, useEffect } from 'react'
import { useIMStore, PLATFORM_LABELS, PLATFORM_ORDER, type Platform, type InstanceConfig } from '../../stores/im'
import { useLocale } from '../../i18n'
import styles from './ImTab.module.css'

function generateId(): string {
  return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`
}

export default function ImTab() {
  const { t } = useLocale()
  const {
    instances, settings, selectedPlatform, loading,
    loadConfigs, saveConfig, deleteConfig,
    startChannel, stopChannel,
    setSelectedPlatform,
  } = useIMStore()

  const [globalEnabled, setGlobalEnabled] = useState(true)
  const [expandedId, setExpandedId] = useState<string | null>(null)

  useEffect(() => {
    loadConfigs()
  }, [])

  const platformInstances = instances.filter(i => i.platform === selectedPlatform)

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
      createdAt: Math.floor(Date.now() / 1000),
      updatedAt: Math.floor(Date.now() / 1000),
    }
    await saveConfig(newConfig)
  }

  return (
    <>
      <div className={styles.header}>
        <div>
          <h3 className={styles.title}>{t('settings.im', 'IM 接入')}</h3>
          <p className={styles.subtitle}>{t('settings.imDesc', '连接手机 IM 平台，让 Agent 在微信/飞书等渠道收发消息')}</p>
        </div>
        <div className={styles.globalToggle}>
          <span className={styles.toggleLabel}>{t('im.globalEnable', '全局启停')}</span>
          <button
            className={`${styles.toggleBtn} ${globalEnabled ? styles.toggleOn : styles.toggleOff}`}
            onClick={() => setGlobalEnabled(!globalEnabled)}
          >
            <span className={styles.toggleKnob} />
          </button>
        </div>
      </div>

      {/* Platform Tabs */}
      <div className={styles.platformTabs}>
        {PLATFORM_ORDER.map(p => (
          <button
            key={p}
            className={`${styles.platformTab} ${selectedPlatform === p ? styles.platformTabActive : ''}`}
            onClick={() => setSelectedPlatform(p)}
          >
            {PLATFORM_LABELS[p]}
          </button>
        ))}
      </div>

      <div className={styles.content}>
        {/* Global Settings Card */}
        <div className={styles.globalCard}>
          <h4 className={styles.cardTitle}>⚙ {t('im.globalSettings', '全局 IM 设置')}</h4>
          <div className={styles.globalRow}>
            <label className={styles.fieldLabel}>
              <span>{t('im.defaultDmPolicy', '默认访问策略')}</span>
              <select className={styles.select} value={settings.defaultDmPolicy} onChange={(e) => {
                // Settings are read-only in this simple version
              }}>
                <option value="pairing">{t('im.pairing', '配对模式 (推荐)')}</option>
                <option value="open">{t('im.open', '开放')}</option>
                <option value="allowlist">{t('im.allowlist', '白名单')}</option>
              </select>
            </label>
            <label className={styles.checkboxLabel}>
              <input type="checkbox" checked={settings.skillsEnabled} readOnly />
              <span>{t('im.skillsEnabled', '启用 Skills')}</span>
            </label>
            <label className={styles.fieldLabel}>
              <span>{t('im.bindAgent', '绑定 Agent')}</span>
              <select className={styles.select} value={settings.defaultAgentId}>
                <option value="main">main (默认)</option>
              </select>
            </label>
          </div>
        </div>

        {/* Instance Cards */}
        {loading ? (
          <div className={styles.loading}>{t('common.loading', '加载中...')}</div>
        ) : platformInstances.length === 0 ? (
          <div className={styles.empty}>
            {t('im.noInstances', `暂无${PLATFORM_LABELS[selectedPlatform]}实例`)}
          </div>
        ) : (
          platformInstances.map(inst => (
            <div key={inst.id} className={styles.instanceCard}>
              <div className={styles.instanceHeader} onClick={() => setExpandedId(expandedId === inst.id ? null : inst.id)}>
                <div className={styles.instanceInfo}>
                  <span className={styles.instanceName}>{inst.instanceName}</span>
                  <span className={`${styles.statusBadge} ${inst.enabled ? styles.statusConnected : styles.statusDisconnected}`}>
                    {inst.enabled ? '● 已连接' : '○ 未连接'}
                  </span>
                  {inst.configJson?.accountId && (
                    <span className={styles.accountId}>{String(inst.configJson.accountId)}</span>
                  )}
                </div>
                <div className={styles.instanceActions}>
                  <button
                    className={`${styles.toggleBtn} ${inst.enabled ? styles.toggleOn : styles.toggleOff}`}
                    onClick={(e) => {
                      e.stopPropagation()
                      if (inst.enabled) {
                        stopChannel(inst.platform, inst.instanceId)
                        saveConfig({ ...inst, enabled: false, updatedAt: Math.floor(Date.now() / 1000) })
                      } else {
                        saveConfig({ ...inst, enabled: true, updatedAt: Math.floor(Date.now() / 1000) }).then(() => {
                          startChannel(inst.platform, inst.instanceId)
                        })
                      }
                    }}
                  >
                    <span className={styles.toggleKnob} />
                  </button>
                  <button className={styles.moreBtn} onClick={(e) => {
                    e.stopPropagation()
                    // More options menu (delete, etc.)
                  }}>⋯</button>
                </div>
              </div>
              {/* Simplified sub-info */}
              <div className={styles.instanceMeta}>
                <span>ID: {inst.instanceId}</span>
              </div>
              {/* Expanded config area — simplified for now, Task 15 will add full ImInstanceCard */}
              {expandedId === inst.id && (
                <div className={styles.expandedConfig}>
                  <div className={styles.configSection}>
                    <label className={styles.fieldLabel}>
                      <span>{t('im.instanceName', '实例名称')}</span>
                      <input
                        className={styles.input}
                        value={inst.instanceName}
                        onChange={(e) => {
                          saveConfig({ ...inst, instanceName: e.target.value, updatedAt: Math.floor(Date.now() / 1000) })
                        }}
                      />
                    </label>
                  </div>
                </div>
              )}
            </div>
          ))
        )}

        {/* Add Instance Button */}
        <button className={styles.addBtn} onClick={handleAddInstance}>
          <span>+</span> {t('im.addInstance', '添加实例')}
        </button>
      </div>
    </>
  )
}
