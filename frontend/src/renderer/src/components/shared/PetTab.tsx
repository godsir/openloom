import { useState, useEffect } from 'react'
import styles from './PetTab.module.css'

type PetSize = 'small' | 'medium' | 'large'
const SIZE_MAP: Record<PetSize, number> = { small: 128, medium: 192, large: 256 }
const SIZE_LABELS: Record<PetSize, string> = { small: '小', medium: '中', large: '大' }
const SIZE_KEYS: PetSize[] = ['small', 'medium', 'large']

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

// PetDex standard states
const PETDEX_STATES: Record<string, number> = {
  idle: 0, runRight: 1, runLeft: 2, wave: 3, jump: 4,
  failed: 5, wait: 6, dash: 7, inspect: 8,
}
const PETDEX_ROW_FRAMES: Record<string, number> = { '0': 6, '1': 8, '2': 8, '3': 4, '4': 5, '5': 8, '6': 6, '7': 6, '8': 6 }

const STATE_LABELS: Record<string, string> = {
  idle: '待机', runRight: '向右跑', runLeft: '向左跑', wave: '挥手', jump: '跳跃',
  failed: '失败', wait: '等待', dash: '奔跑', inspect: '审视',
  // Boba-specific labels
  talking: '回复', working: '工作中', thinking: '思考中', happy: '完成', error: '错误',
}

const bc = new BroadcastChannel('pet')

export default function PetTab() {
  const [enabled, setEnabled] = useState(false)
  const [size, setSize] = useState<PetSize>('small')
  const [petsDir, setPetsDir] = useState('')
  const [pets, setPets] = useState<PetMeta[]>([])
  const [activePetId, setActivePetId] = useState('homelander-2')
  const [activeState, setActiveState] = useState<string | null>(null)
  const [ready, setReady] = useState(false)

  useEffect(() => {
    Promise.all([
      window.loom.getPreference('petEnabled', false),
      window.loom.getPreference('petSize', 'small'),
      window.loom.getPreference('activePetId', 'homelander-2'),
      window.loom.getLoomDir(),
      window.loom.listPets(),
    ]).then(([on, sz, petId, dir, petList]: [boolean, string, string, string, PetMeta[]]) => {
      setEnabled(on)
      setSize(sz as PetSize)
      setActivePetId(petId)
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

  const broadcastSize = (sz: PetSize) => {
    bc.postMessage({ type: 'size', size: SIZE_MAP[sz] })
  }

  const toggle = (on: boolean) => {
    setEnabled(on)
    window.loom.setPreference('petEnabled', on)
    window.loom.togglePet(on)
    if (on) broadcastSize(size)
  }

  const changeSize = (sz: PetSize) => {
    setSize(sz)
    window.loom.setPreference('petSize', sz)
    broadcastSize(sz)
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

  if (!ready) return null

  return (
    <div className={styles.container}>
      {/* Enable/Disable */}
      <div className={styles.card}>
        <label className={styles.switchRow}>
          <span className={styles.switchLabel}>启用桌宠</span>
          <button
            className={`${styles.toggle} ${enabled ? styles.toggleOn : ''}`}
            onClick={() => toggle(!enabled)}
          >
            <span className={styles.toggleKnob} />
          </button>
        </label>
        <p className={styles.cardDesc}>基于 Petdex 精灵图格式，兼容 Codex 宠物生态</p>
      </div>

      {/* Pet list */}
      {pets.length > 0 && (
        <div className={styles.card}>
          <h4 className={styles.cardTitle}>宠物列表</h4>
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
        </div>
      )}

      {/* Active pet info */}
      {currentPet && (
        <div className={styles.card}>
          <h4 className={styles.cardTitle}>当前宠物</h4>
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
                {cols} x {rows} 格
                <span className={styles.specSep}>/</span>
                {states.length} 状态
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Size */}
      <div className={styles.card}>
        <h4 className={styles.cardTitle}>显示大小</h4>
        <div className={styles.segRow}>
          {SIZE_KEYS.map(sz => (
            <button
              key={sz}
              className={`${styles.segBtn} ${size === sz ? styles.segBtnActive : ''}`}
              onClick={() => changeSize(sz)}
            >
              {SIZE_LABELS[sz]} ({SIZE_MAP[sz]}px)
            </button>
          ))}
        </div>
      </div>

      {/* Pets directory */}
      {petsDir && (
        <div className={styles.card}>
          <h4 className={styles.cardTitle}>宠物目录</h4>
          <p className={styles.dirPath}>{petsDir}</p>
          <p className={styles.cardDesc}>
            将 Petdex 宠物放入该目录即可自动识别
          </p>
        </div>
      )}

      {/* Animation States */}
      {states.length > 0 && (
        <div className={styles.card}>
          <h4 className={styles.cardTitle}>动画状态</h4>
          <div className={styles.stateTable}>
            <div className={styles.stateHeader}>
              <span>状态</span>
              <span>行</span>
              <span>帧数</span>
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
                    {isActive ? '播放中' : '测试'}
                  </button>
                </div>
              )
            })}
          </div>
        </div>
      )}
    </div>
  )
}
