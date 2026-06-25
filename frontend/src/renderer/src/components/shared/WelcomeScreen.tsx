import { useStore } from '../../stores'
import { IconCpu, IconTerminal, IconBrain, IconBookOpen, IconSparkles, IconMessageSquare } from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './WelcomeScreen.module.css'
import bannerImg from '@asset/banner.png'

export default function WelcomeScreen() {
  const { t } = useLocale()
  const switchSession = useStore((s) => s.switchSession)
  const sessions = useStore((s) => s.sessions)

  const FEATURES = [
    { label: t('welcome.featureMultiModel'), icon: IconCpu },
    { label: t('welcome.featureMcpTools'), icon: IconTerminal },
    { label: t('welcome.featureKnowledgeGraph'), icon: IconBrain },
    { label: t('welcome.featureLspCode'), icon: IconBookOpen },
    { label: t('welcome.featureSkills'), icon: IconSparkles },
  ]

  // Recent non-empty sessions, newest first, max 3
  const recentSessions = sessions
    .filter(s => s.title || s.firstMessage)
    .sort((a, b) => (b.modified || '').localeCompare(a.modified || ''))
    .slice(0, 3)

  return (
    <div className={styles.wrapper}>
      <div className={styles.inner}>
        <div className={styles.banner}>
          <img
            src={bannerImg}
            alt="openLoom"
            className={styles.bannerImg}
          />
        </div>
        <p className={styles.subtitle}>{t('welcome.subtitle')}</p>

        <div className={styles.features}>
          {FEATURES.map(({ label, icon: Icon }) => (
            <span key={label} className={styles.featurePill}>
              <Icon size={11} />
              {label}
            </span>
          ))}
        </div>

        {recentSessions.length > 0 && (
          <div className={styles.recentSection}>
            <div className={styles.recentLabel}>{t('welcome.recentChats')}</div>
            <div className={styles.recentList}>
              {recentSessions.map((s) => (
                <button
                  key={s.path}
                  onClick={() => switchSession(s.path)}
                  className={styles.recentItem}
                >
                  <IconMessageSquare size={12} className={styles.recentIcon} />
                  <span className={styles.recentTitle}>{s.title || t('welcome.unnamedChat')}</span>
                  <span className={styles.recentCount}>{s.messageCount}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        <p className={styles.footer}>
          {t('welcome.footer')}
        </p>
      </div>
    </div>
  )
}
