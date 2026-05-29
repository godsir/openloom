import { useState } from 'react'
import styles from './Onboarding.module.css'
import logoDev from '../../assets/loom_logo_dev.png'
import logoRelease from '../../assets/loom_logo.png'

interface Step {
  title: string
  description: string
  tags: string[]
}

const STEPS: Step[] = [
  {
    title: '欢迎来到 openLoom',
    description: '本地优先的私人 AI 助理，所有数据存储在你的电脑上。',
    tags: ['多模型支持', 'MCP 工具', '知识图谱记忆', 'LSP 代码理解', 'Skills 技能'],
  },
  {
    title: '选择你的模型',
    description: '支持云端与本地模型，在设置中配置后即可按需切换。',
    tags: ['Anthropic', 'OpenAI', 'DeepSeek', 'LM Studio', 'Ollama'],
  },
  {
    title: '开始对话',
    description: '创建你的第一个对话，在左侧边栏管理会话和主题。',
    tags: [],
  },
]

export default function Onboarding({
  onComplete,
}: {
  onComplete: () => void
}) {
  const [step, setStep] = useState(0)
  const isPackaged = window.__isPackaged__ ?? true

  return (
    <div className={styles.backdrop}>
      <div className={`${styles.bgOrb} ${styles.bgOrb1}`} />
      <div className={`${styles.bgOrb} ${styles.bgOrb2}`} />
      <div className={`${styles.bgOrb} ${styles.bgOrb3}`} />
      <div className={styles.bgGrid} />

      <div className={styles.card}>
        <div className={styles.logoBox}>
          <img
            src={isPackaged ? logoRelease : logoDev}
            alt="openLoom"
            className={styles.logoImg}
          />
        </div>

        <div className={styles.dots}>
          {STEPS.map((_, i) => (
            <div
              key={i}
              className={`${styles.dot} ${i <= step ? styles.dotActive : styles.dotInactive}`}
            />
          ))}
        </div>

        <div key={step} className={styles.stepContent}>
          <h2 className={styles.title}>{STEPS[step].title}</h2>
          <p className={styles.desc}>{STEPS[step].description}</p>

          <div className={styles.tags}>
            {STEPS[step].tags.map((name) => (
              <span key={name} className={styles.tag}>
                {name}
              </span>
            ))}
          </div>
        </div>

        <div className={styles.actions}>
          {step > 0 && (
            <button
              onClick={() => setStep(step - 1)}
              className={styles.btnSecondary}
            >
              上一步
            </button>
          )}
          {step < STEPS.length - 1 ? (
            <button
              onClick={() => setStep(step + 1)}
              className={styles.btnPrimary}
            >
              下一步
            </button>
          ) : (
            <button onClick={onComplete} className={styles.btnPrimary}>
              开始使用
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
