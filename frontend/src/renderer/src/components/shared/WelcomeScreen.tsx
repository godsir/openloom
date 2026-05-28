import { useStore } from '../../stores'
import { IconPlus } from '../../utils/icons'
import styles from './WelcomeScreen.module.css'
import logoDev from '../../assets/loom_logo_dev.png'
import logoRelease from '../../assets/loom_logo.png'

export default function WelcomeScreen() {
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)
  const isPackaged = window.__isPackaged__ ?? true

  const handleStart = async () => {
    const id = await createSession()
    if (id) await switchSession(id)
  }

  return (
    <div className={styles.wrapper}>
      <div className={styles.inner}>
        <div className={styles.logo}>
          <img
            src={isPackaged ? logoRelease : logoDev}
            alt="openLoom"
            className={styles.logoImg}
          />
        </div>
        <h1 className={styles.title}>openLoom</h1>
        <p className={styles.subtitle}>你的私人 AI 助理</p>

        <div className={styles.features}>
          {['多模型支持', 'MCP 工具', '知识图谱记忆', 'LSP 代码理解', 'Skills 技能'].map(name => (
            <span key={name} className="pill-neutral">{name}</span>
          ))}
        </div>

        <button onClick={handleStart} className={styles.startBtn}>
          <IconPlus size={13} />
          开始新对话
        </button>
        <p className={styles.footer}>
          所有数据存储在本地 SQLite 数据库中 · 完全离线可用
        </p>
      </div>
    </div>
  )
}
