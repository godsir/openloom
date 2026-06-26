import { useState } from 'react'
import { useIMStore, type InstanceConfig, type Platform } from '../../stores/im'
import { useLocale } from '../../i18n'
import ImConnectivityTest from './ImConnectivityTest'
import styles from './ImTab.module.css'

interface Props {
  config: InstanceConfig
}

export default function ImInstanceCard({ config }: Props) {
  const { t } = useLocale()
  const { saveConfig, deleteConfig, startChannel, stopChannel } = useIMStore()
  const [expanded, setExpanded] = useState(false)
  const [showConnectivityTest, setShowConnectivityTest] = useState(false)

  const handleToggle = async (e: React.MouseEvent) => {
    e.stopPropagation()
    if (config.enabled) {
      await stopChannel(config.platform, config.instanceId)
      await saveConfig({ ...config, enabled: false, updatedAt: Math.floor(Date.now() / 1000) })
    } else {
      await saveConfig({ ...config, enabled: true, updatedAt: Math.floor(Date.now() / 1000) })
      await startChannel(config.platform, config.instanceId)
    }
  }

  const handleDelete = async (e: React.MouseEvent) => {
    e.stopPropagation()
    if (confirm(`确定删除实例 "${config.instanceName}"？`)) {
      await deleteConfig(config.platform, config.instanceId)
    }
  }

  return (
    <div className={styles.instanceCard}>
      {/* Header row — always visible */}
      <div className={styles.instanceHeader} onClick={() => setExpanded(!expanded)}>
        <div className={styles.instanceInfo}>
          <span className={styles.instanceName}>{config.instanceName}</span>
          <span className={`${styles.statusBadge} ${config.enabled ? styles.statusConnected : styles.statusDisconnected}`}>
            {config.enabled ? '● 已连接' : '○ 未连接'}
          </span>
          {config.configJson?.accountId && (
            <span className={styles.accountId}>{String(config.configJson.accountId)}</span>
          )}
        </div>
        <div className={styles.instanceActions}>
          <button className={styles.actionBtn} onClick={(e) => { e.stopPropagation(); setShowConnectivityTest(true) }}>
            连接测试
          </button>
          <button
            className={`${styles.toggleBtn} ${config.enabled ? styles.toggleOn : styles.toggleOff}`}
            onClick={handleToggle}
          >
            <span className={styles.toggleKnob} />
          </button>
          <button className={styles.moreBtn} onClick={handleDelete}>⋯</button>
        </div>
      </div>

      {/* Runtime stats */}
      <div className={styles.instanceMeta}>
        <span>最后收消息: —</span>
        <span>最后发消息: —</span>
      </div>

      {/* Expanded config panel */}
      {expanded && (
        <div className={styles.expandedConfig}>
          {/* Basic Info */}
          <div className={styles.configSection}>
            <h5 className={styles.sectionTitle}>基本信息</h5>
            <div className={styles.row}>
              <label className={styles.fieldLabel}>
                <span>实例名称</span>
                <input
                  className={styles.input}
                  value={config.instanceName}
                  onChange={(e) => saveConfig({ ...config, instanceName: e.target.value, updatedAt: Math.floor(Date.now() / 1000) })}
                />
              </label>
              <label className={styles.fieldLabel}>
                <span>Instance ID</span>
                <input className={styles.input} value={config.instanceId} disabled />
              </label>
            </div>
          </div>

          {/* DM Policy */}
          <div className={styles.configSection}>
            <h5 className={styles.sectionTitle}>💬 私聊 (DM) 策略</h5>
            <div className={styles.policyGrid}>
              {(['pairing', 'allowlist', 'open', 'disabled'] as const).map(policy => (
                <button
                  key={policy}
                  className={`${styles.policyCard} ${config.dmPolicy === policy ? styles.policyActive : ''}`}
                  onClick={() => saveConfig({ ...config, dmPolicy: policy, updatedAt: Math.floor(Date.now() / 1000) })}
                >
                  {policy === 'pairing' && '🔐 配对模式'}
                  {policy === 'allowlist' && '📋 白名单'}
                  {policy === 'open' && '🌐 开放'}
                  {policy === 'disabled' && '🚫 禁用'}
                </button>
              ))}
            </div>
          </div>

          {/* Group Policy */}
          <div className={styles.configSection}>
            <h5 className={styles.sectionTitle}>👥 群聊策略</h5>
            <div className={styles.policyGrid}>
              {(['allowlist', 'open', 'disabled'] as const).map(policy => (
                <button
                  key={policy}
                  className={`${styles.policyCard} ${config.groupPolicy === policy ? styles.policyActive : ''}`}
                  onClick={() => saveConfig({ ...config, groupPolicy: policy, updatedAt: Math.floor(Date.now() / 1000) })}
                >
                  {policy === 'allowlist' && '📋 白名单'}
                  {policy === 'open' && '🌐 开放'}
                  {policy === 'disabled' && '🚫 禁用'}
                </button>
              ))}
            </div>
          </div>

          {/* Save / Discard */}
          <div className={styles.configActions}>
            <button className={styles.discardBtn} onClick={handleDelete}>丢弃</button>
            <button className={styles.saveBtn} onClick={() => saveConfig({ ...config, updatedAt: Math.floor(Date.now() / 1000) })}>保存</button>
          </div>
        </div>
      )}

      {/* Connectivity Test Modal */}
      {showConnectivityTest && (
        <ImConnectivityTest
          platform={config.platform}
          instanceId={config.instanceId}
          onClose={() => setShowConnectivityTest(false)}
        />
      )}
    </div>
  )
}
