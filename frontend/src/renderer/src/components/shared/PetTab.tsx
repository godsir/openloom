import { useState, useEffect } from 'react'
import { useLocale } from '../../i18n'
import settingsStyles from './SettingsModal.module.css'
import styles from './PetTab.module.css'

type PetSize = 'small' | 'medium' | 'large'
const SIZE_MAP: Record<PetSize, number> = { small: 128, medium: 192, large: 256 }
const SIZE_KEYS: PetSize[] = ['small', 'medium', 'large']

function useSizeLabels(): Record<PetSize, string> {
  const { t } = useLocale()
  return { small: t('pet.sizeSmallLabel'), medium: t('pet.sizeMediumLabel'), large: t('pet.sizeLargeLabel') }
}

function useIdleIntervals(): { value: number; label: string }[] {
  const { t } = useLocale()
  return [
    { value: 15, label: t('pet.idle15s') },
    { value: 30, label: t('pet.idle30s') },
    { value: 60, label: t('pet.idle1m') },
    { value: 120, label: t('pet.idle2m') },
    { value: 300, label: t('pet.idle5m') },
  ]
}

function useBreakIntervals(): { value: number; label: string }[] {
  const { t } = useLocale()
  return [
    { value: 0, label: t('common.disable') },
    { value: 25, label: t('pet.break25m') },
    { value: 45, label: t('pet.break45m') },
    { value: 60, label: t('pet.break60m') },
  ]
}

interface PetMeta {
  id: string
  displayName: string
  description: string
  spritesheetPath: string
  frameWidth?: number
  frameHeight?: number
  columns?: number
  rows?: number
  framesPerRow?: number
  rowFrames?: Record<string, number>
  states?: Record<string, number>
}

const PETDEX_STATES: Record<string, number> = {
  idle: 0, runRight: 1, runLeft: 2, wave: 3, jump: 4,
  failed: 5, wait: 6, dash: 7, inspect: 8,
}
const PETDEX_ROW_FRAMES: Record<string, number> = { '0': 6, '1': 8, '2': 8, '3': 4, '4': 5, '5': 8, '6': 6, '7': 6, '8': 6 }

function useStateLabels(): Record<string, string> {
  const { t } = useLocale()
  return {
    idle: t('pet.stateIdle'), runRight: t('pet.stateRunRight'), runLeft: t('pet.stateRunLeft'),
    wave: t('pet.stateWave'), jump: t('pet.stateJump'),
    failed: t('pet.stateFailed'), wait: t('pet.stateWait'), dash: t('pet.stateDash'), inspect: t('pet.stateInspect'),
    talking: t('pet.stateTalking'), working: t('pet.stateWorking'), thinking: t('pet.stateThinking'),
    happy: t('pet.stateHappy'), error: t('pet.stateError'),
  }
}

const bc = new BroadcastChannel('pet')

export default function PetTab() {
  const { t } = useLocale()
  const SIZE_LABELS = useSizeLabels()
  const IDLE_INTERVALS = useIdleIntervals()
  const BREAK_INTERVALS = useBreakIntervals()
  const STATE_LABELS = useStateLabels()

  const [enabled, setEnabled] = useState(false)
  const [size, setSize] = useState<PetSize>('small')
  const [petsDir, setPetsDir] = useState('')
  const [pets, setPets] = useState<PetMeta[]>([])
  const [activePetId, setActivePetId] = useState('homelander-2')
  const [activeState, setActiveState] = useState<string | null>(null)
  const [idleInterval, setIdleInterval] = useState(30)
  const [breakInterval, setBreakInterval] = useState(0)
  const [ready, setReady] = useState(false)

  useEffect(() => {
    Promise.all([
      window.loom.getPreference('petEnabled', false),
      window.loom.getPreference('petSize', 'small'),
      window.loom.getPreference('activePetId', 'homelander-2'),
      window.loom.getPreference('petIdleInterval', 30),
      window.loom.getPreference('petBreakInterval', 0),
      window.loom.getLoomDir(),
      window.loom.listPets(),
    ]).then(([on, sz, petId, idleInt, breakInt, dir, petList]: [boolean, string, string, number, number, string, PetMeta[]]) => {
      setEnabled(on)
      setSize(sz as PetSize)
      setActivePetId(petId)
      setIdleInterval(idleInt)
      setBreakInterval(breakInt)
      setPetsDir(dir + '/pets')
      setPets(petList)
      setReady(true)
      bc.postMessage({ type: 'size', size: SIZE_MAP[sz as PetSize] })
    }).catch(() => setReady(true))
  }, [])

  const currentPet = pets.find(p => p.id === activePetId)
  const states = currentPet?.states ? Object.entries(currentPet.states) : Object.entries(PETDEX_STATES)
  const rowFrames: Record<string, number> = currentPet?.rowFrames || PETDEX_ROW_FRAMES
  const frameW = currentPet?.frameWidth ?? 192
  const frameH = currentPet?.frameHeight ?? 208
  const cols = currentPet?.columns ?? 9
  const rows = currentPet?.rows ?? 8

  const broadcastSize = (sz: PetSize) => bc.postMessage({ type: 'size', size: SIZE_MAP[sz] })

  const toggle = (on: boolean) => {
    setEnabled(on)
    window.loom.setPreference('petEnabled', on)
    window.loom.togglePet(on)
    if (on) {
      broadcastSize(size)
      window.loom.resizePet(SIZE_MAP[size])
    }
  }

  const changeSize = (sz: PetSize) => {
    setSize(sz)
    window.loom.setPreference('petSize', sz)
    broadcastSize(sz)
    window.loom.resizePet(SIZE_MAP[sz])
  }

  const selectPet = (id: string) => {
    setActivePetId(id)
    window.loom.setPreference('activePetId', id)
    bc.postMessage({ type: 'pet', petId: id })
  }

  const testAnim = (s: string) => {
    setActiveState(s)
    bc.postMessage({ type: 'state', state: s })
  }

  const changeIdleInterval = (val: number) => {
    setIdleInterval(val)
    window.loom.setPreference('petIdleInterval', val)
    bc.postMessage({ type: 'config', idleInterval: val })
  }

  const changeBreakInterval = (val: number) => {
    setBreakInterval(val)
    window.loom.setPreference('petBreakInterval', val)
    bc.postMessage({ type: 'config', breakInterval: val })
  }

  if (!ready) return <p className={settingsStyles.toolsEmpty}>{t('common.loading')}</p>

  return (
    <div className={settingsStyles.aboutSection}>
      {/* Toggle */}
      <div className={settingsStyles.themeLabel}>{t('pet.title')}</div>

      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>{t('pet.enablePet')}</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>
            {t('pet.enablePetDesc')}
          </p>
        </div>
        <div className={settingsStyles.mcpTransportToggle}>
          <button
            className={`${settingsStyles.mcpTransportBtn} ${enabled ? settingsStyles.mcpTransportActive : ''}`}
            onClick={() => toggle(true)}
          >
            {t('software.enable')}
          </button>
          <button
            className={`${settingsStyles.mcpTransportBtn} ${!enabled ? settingsStyles.mcpTransportActive : ''}`}
            onClick={() => toggle(false)}
          >
            {t('software.disable')}
          </button>
        </div>
      </div>

      <hr className={settingsStyles.sectionDivider} />

      {/* Pet List */}
      {pets.length > 0 && (
        <>
          <div className={settingsStyles.themeLabel}>{t('pet.petList')}</div>
          <div className={styles.petList}>
            {pets.map(pet => (
              <button
                key={pet.id}
                className={`${styles.petItem} ${pet.id === activePetId ? styles.petItemActive : ''}`}
                onClick={() => selectPet(pet.id)}
              >
                <div
                  className={styles.petThumb}
                  style={{
                    backgroundImage: `url(loom-pet://${pet.id}/${pet.spritesheetPath || 'spritesheet.webp'})`,
                    backgroundSize: `${(pet.columns || 9) * 40}px auto`,
                  }}
                />
                <div className={styles.petItemInfo}>
                  <div className={styles.petItemName}>{pet.displayName}</div>
                  <div className={styles.petItemDesc}>{pet.description}</div>
                </div>
              </button>
            ))}
          </div>

          {currentPet && (
            <div className={styles.petInfo}>
              <div className={styles.petPreview}>
                <div
                  className={styles.petThumbLarge}
                  style={{
                    backgroundImage: `url(loom-pet://${currentPet.id}/${currentPet.spritesheetPath || 'spritesheet.webp'})`,
                    backgroundSize: `${(currentPet.columns || 9) * 72}px auto`,
                  }}
                />
              </div>
              <div className={styles.petMeta}>
                <div className={styles.petName}>{currentPet.displayName}</div>
                <div className={styles.petDesc}>{currentPet.description}</div>
                <div className={styles.petSpecs}>
                  {frameW} x {frameH}
                  <span className={styles.specSep}>/</span>
                  {cols} x {rows} {t('pet.gridCell')}
                  <span className={styles.specSep}>/</span>
                  {t('pet.stateCount', { n: states.length })}
                </div>
              </div>
            </div>
          )}
          <hr className={settingsStyles.sectionDivider} />
        </>
      )}

      {/* Display Size */}
      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>{t('pet.displaySize')}</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('pet.displaySizeDesc')}</p>
        </div>
        <div className={settingsStyles.mcpTransportToggle}>
          {SIZE_KEYS.map(sz => (
            <button
              key={sz}
              className={`${settingsStyles.mcpTransportBtn} ${size === sz ? settingsStyles.mcpTransportActive : ''}`}
              onClick={() => changeSize(sz)}
            >
              {SIZE_LABELS[sz]} ({SIZE_MAP[sz]}px)
            </button>
          ))}
        </div>
      </div>

      <hr className={settingsStyles.sectionDivider} />

      {/* Idle Interval */}
      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>{t('pet.idleInterval')}</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('pet.idleIntervalDesc')}</p>
        </div>
        <div className={settingsStyles.mcpTransportToggle}>
          {IDLE_INTERVALS.map(iv => (
            <button
              key={iv.value}
              className={`${settingsStyles.mcpTransportBtn} ${idleInterval === iv.value ? settingsStyles.mcpTransportActive : ''}`}
              onClick={() => changeIdleInterval(iv.value)}
            >
              {iv.label}
            </button>
          ))}
        </div>
      </div>

      <hr className={settingsStyles.sectionDivider} />

      {/* Break Reminder */}
      <div className={settingsStyles.aboutRow}>
        <div>
          <span className={settingsStyles.aboutLabel}>{t('pet.breakReminder')}</span>
          <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>{t('pet.breakReminderDesc')}</p>
        </div>
        <div className={settingsStyles.mcpTransportToggle}>
          {BREAK_INTERVALS.map(iv => (
            <button
              key={iv.value}
              className={`${settingsStyles.mcpTransportBtn} ${breakInterval === iv.value ? settingsStyles.mcpTransportActive : ''}`}
              onClick={() => changeBreakInterval(iv.value)}
            >
              {iv.label}
            </button>
          ))}
        </div>
      </div>

      {/* Pet Directory */}
      {petsDir && (
        <>
          <hr className={settingsStyles.sectionDivider} />
          <div className={settingsStyles.aboutRow}>
            <div>
              <span className={settingsStyles.aboutLabel}>{t('pet.petDir')}</span>
              <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0, fontFamily: 'var(--font-mono)' }}>
                {petsDir}
              </p>
            </div>
          </div>
        </>
      )}

      {/* Animation States */}
      {states.length > 0 && (
        <>
          <hr className={settingsStyles.sectionDivider} />
          <div className={settingsStyles.themeLabel}>{t('pet.animStates')}</div>
          <div className={styles.stateTable}>
            <div className={styles.stateHeader}>
              <span>{t('pet.state')}</span>
              <span>{t('pet.row')}</span>
              <span>{t('pet.frames')}</span>
              <span />
            </div>
            {states.map(([name, row]) => {
              const frames = rowFrames[String(row)] ?? currentPet?.framesPerRow ?? 6
              const label = STATE_LABELS[name] || name
              const isActive = activeState === name
              return (
                <div key={name} className={`${styles.stateRow} ${isActive ? styles.stateRowActive : ''}`}>
                  <span className={styles.stateName}>{label}</span>
                  <span className={styles.stateMono}>{row}</span>
                  <span className={styles.stateMono}>{frames}</span>
                  <button className={styles.playBtn} onClick={() => testAnim(name)}>
                    {isActive ? t('pet.playing') : t('pet.test')}
                  </button>
                </div>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}
