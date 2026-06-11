import { useState, useCallback } from 'react'
import { useLocale, t as _t } from '../../i18n'
import {
  keybindingRegistry,
  keyStringToDisplay,
  CATEGORY_LABEL_I18N,
  type ResolvedCommand,
  type CommandCategory,
} from '../../services/keybindings'
import { KeyCaptureModal } from '../shared/KeyCaptureModal'
import { IconRotateCcw } from '../../utils/icons'
import shared from '../shared/SettingsModal.module.css'
import styles from './ShortcutsTab.module.css'

export default function ShortcutsTab() {
  const { t } = useLocale()
  const [commands, setCommands] = useState<ResolvedCommand[]>(
    () => keybindingRegistry.getResolvedCommands()
  )
  const [captureTarget, setCaptureTarget] = useState<ResolvedCommand | null>(null)
  const [conflictCmd, setConflictCmd] = useState<ResolvedCommand | null>(null)

  const refresh = useCallback(() => {
    setCommands(keybindingRegistry.getResolvedCommands())
  }, [])

  const handleResetAll = useCallback(async () => {
    await keybindingRegistry.resetAll()
    refresh()
  }, [refresh])

  const handleRebind = useCallback(
    async (newKeys: string) => {
      if (!captureTarget) return

      const conflictId = await keybindingRegistry.rebind(captureTarget.id, newKeys)
      if (conflictId) {
        const conflict = commands.find((c) => c.id === conflictId)
        setConflictCmd(conflict || null)
        refresh()
      } else {
        setCaptureTarget(null)
        setConflictCmd(null)
        refresh()
      }
    },
    [captureTarget, commands, refresh],
  )

  const handleOverride = useCallback(
    async (newKeys: string) => {
      if (!captureTarget) return
      if (conflictCmd) {
        await keybindingRegistry.reset(conflictCmd.id)
      }
      await keybindingRegistry.rebind(captureTarget.id, newKeys)
      setCaptureTarget(null)
      setConflictCmd(null)
      refresh()
    },
    [captureTarget, conflictCmd, refresh],
  )

  const handleClear = useCallback(async () => {
    if (!captureTarget) return
    await keybindingRegistry.rebind(captureTarget.id, '')
    setCaptureTarget(null)
    setConflictCmd(null)
    refresh()
  }, [captureTarget, refresh])

  const handleResetSingle = useCallback(
    async (commandId: string) => {
      await keybindingRegistry.reset(commandId)
      refresh()
    },
    [refresh],
  )

  // Group commands by category
  const grouped = new Map<CommandCategory, ResolvedCommand[]>()
  for (const cmd of commands) {
    const list = grouped.get(cmd.category) || []
    list.push(cmd)
    grouped.set(cmd.category, list)
  }

  return (
    <>
      <div className={shared.contentHeader}>
        <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between' }}>
          <div>
            <h3 className={shared.sectionTitle}>{t('keybindings.title')}</h3>
            <p className={shared.sectionDesc}>{t('keybindings.description')}</p>
          </div>
          <button className={styles.resetAllBtn} onClick={handleResetAll}>
            <IconRotateCcw size={12} />
            {t('keybindings.resetAll')}
          </button>
        </div>
      </div>

      <div className={shared.contentBody}>

        {Array.from(grouped.entries()).map(([category, cmds]) => (
          <div key={category}>
            <h4 className={styles.categoryTitle}>
              {t(CATEGORY_LABEL_I18N[category])}
            </h4>
            {cmds.map((cmd) => {
              const isDefault = cmd.currentKeys === cmd.defaultKeys
              return (
                <div key={cmd.id} className={styles.commandRow}>
                  <div className={styles.commandInfo}>
                    <p className={styles.commandLabel}>{t(cmd.labelKey)}</p>
                    <p className={styles.commandDesc}>{t(cmd.descKey)}</p>
                  </div>
                  <button
                    className={`${styles.keyPill} ${!cmd.currentKeys ? styles.keyPillEmpty : ''}`}
                    onClick={() => {
                      setCaptureTarget(cmd)
                      setConflictCmd(null)
                    }}
                    title={t('keybindings.clickToChange')}
                  >
                    {cmd.currentKeys
                      ? keyStringToDisplay(cmd.currentKeys)
                      : t('keybindings.disabled')}
                  </button>
                  {!isDefault && (
                    <button
                      className={styles.resetBtn}
                      onClick={() => handleResetSingle(cmd.id)}
                      title={t('keybindings.resetToDefault')}
                    >
                      <IconRotateCcw size={12} />
                    </button>
                  )}
                </div>
              )
            })}
            <div className={styles.divider} />
          </div>
        ))}

        {commands.length === 0 && (
          <p className={styles.empty}>{t('keybindings.noShortcuts')}</p>
        )}
      </div>

      {captureTarget && (
        <KeyCaptureModal
          commandLabel={t(captureTarget.labelKey)}
          currentKeys={keyStringToDisplay(captureTarget.currentKeys)}
          conflictLabel={
            conflictCmd ? t(conflictCmd.labelKey) : null
          }
          onConfirm={
            conflictCmd ? handleOverride : handleRebind
          }
          onCancel={() => {
            setCaptureTarget(null)
            setConflictCmd(null)
          }}
          onClear={handleClear}
        />
      )}
    </>
  )
}
