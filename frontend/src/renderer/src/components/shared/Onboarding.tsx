import { useState } from 'react'
import { useLocale } from '../../i18n'
import styles from './Onboarding.module.css'
import logoDev from '../../assets/loom_logo_dev.png'
import logoRelease from '../../assets/loom_logo.png'

interface Step {
  title: string
  description: string
  tags: string[]
}

function useSteps(): Step[] {
  const { t } = useLocale()
  return [
    {
      title: t('onboarding.welcomeTitle'),
      description: t('onboarding.welcomeDesc'),
      tags: [t('onboarding.multiModel'), t('onboarding.mcpTools'), t('onboarding.kgMemory'), t('onboarding.lspCode'), t('onboarding.skillsSystem')],
    },
    {
      title: t('onboarding.chooseModel'),
      description: t('onboarding.chooseModelDesc'),
      tags: ['Anthropic', 'OpenAI', 'DeepSeek', 'LM Studio', 'Ollama'],
    },
    {
      title: t('onboarding.startChat'),
      description: t('onboarding.startChatDesc'),
      tags: [],
    },
  ]
}

export default function Onboarding({
  onComplete,
}: {
  onComplete: () => void
}) {
  const { t } = useLocale()
  const [step, setStep] = useState(0)
  const isPackaged = window.__isPackaged__ ?? true
  const STEPS = useSteps()

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
              {t('onboarding.prev')}
            </button>
          )}
          {step < STEPS.length - 1 ? (
            <button
              onClick={() => setStep(step + 1)}
              className={styles.btnPrimary}
            >
              {t('onboarding.next')}
            </button>
          ) : (
            <button onClick={onComplete} className={styles.btnPrimary}>
              {t('onboarding.getStarted')}
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
