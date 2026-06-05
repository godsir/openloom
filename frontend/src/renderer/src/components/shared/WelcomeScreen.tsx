import { useStore } from '../../stores'
import { IconPlus, IconCpu, IconTerminal, IconBrain, IconBookOpen, IconSparkles, IconMessageSquare } from '../../utils/icons'
import styles from './WelcomeScreen.module.css'
import logoDev from '../../assets/loom_logo_dev.png'
import logoRelease from '../../assets/loom_logo.png'

const FEATURES = [
  { label: '多模型支持', icon: IconCpu },
  { label: 'MCP 工具', icon: IconTerminal },
  { label: '知识图谱', icon: IconBrain },
  { label: 'LSP 代码理解', icon: IconBookOpen },
  { label: 'Skills 技能', icon: IconSparkles },
]

export default function WelcomeScreen() {
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)
  const sessions = useStore((s) => s.sessions)
  const isPackaged = window.__isPackaged__ ?? true

  const handleStart = async () => {
    const id = await createSession()
    if (id) await switchSession(id)
  }

  // Recent non-empty sessions, newest first, max 5
  const recentSessions = sessions
    .filter(s => s.title || s.firstMessage)
    .sort((a, b) => (b.modified || '').localeCompare(a.modified || ''))
    .slice(0, 3)

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
        <p className={styles.subtitle}>本地优先的私人 AI 助理</p>

        <div className={styles.features}>
          {FEATURES.map(({ label, icon: Icon }) => (
            <span key={label} className={styles.featurePill}>
              <Icon size={11} />
              {label}
            </span>
          ))}
        </div>

        <button onClick={handleStart} className={styles.startBtn}>
          <IconPlus size={14} />
          开始新对话
        </button>

        {recentSessions.length > 0 && (
          <div className={styles.recentSection}>
            <div className={styles.recentLabel}>最近对话</div>
            <div className={styles.recentList}>
              {recentSessions.map((s) => (
                <button
                  key={s.path}
                  onClick={() => switchSession(s.path)}
                  className={styles.recentItem}
                >
                  <IconMessageSquare size={12} className={styles.recentIcon} />
                  <span className={styles.recentTitle}>{s.title || '未命名对话'}</span>
                  <span className={styles.recentCount}>{s.messageCount}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        <p className={styles.footer}>
          所有数据存储在本地 SQLite 数据库中 · 完全离线可用
        </p>
      </div>
    </div>
  )
}
