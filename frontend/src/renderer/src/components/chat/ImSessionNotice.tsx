import type { Platform } from '../../stores/im'
import PlatformIcon from '../shared/PlatformIcon'
import styles from '../input/InputArea.module.css'

const PLATFORM_LABELS: Record<Platform, string> = {
  telegram: 'Telegram',
  feishu: 'Feishu',
  wechat: '微信',
  wecom: '企业微信',
  dingtalk: '钉钉',
  qq: 'QQ',
  discord: 'Discord',
  popo: 'POPO',
}

interface Props {
  platform: Platform
  /** Renderer-side i18n t-function. */
  t: (key: string, vars?: Record<string, string | number>) => string
}

export default function ImSessionNotice({ platform, t }: Props) {
  const label = PLATFORM_LABELS[platform] ?? platform

  return (
    <div className={styles.wrapper}>
      <div className={styles.container} style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', padding: '32px 0', gap: 12 }}>
        <PlatformIcon platform={platform} size={32} />
        <div style={{ textAlign: 'center' }}>
          <div style={{ fontSize: 14, fontWeight: 500, color: 'var(--text-primary)', marginBottom: 4 }}>
            {t('im.sessionNoticeTitle', '此会话来自 {platform}').replace('{platform}', label)}
          </div>
          <div style={{ fontSize: 13, color: 'var(--text-muted)' }}>
            {t('im.sessionNoticeBody', '请在 {platform} 中继续对话').replace('{platform}', label)}
          </div>
        </div>
      </div>
    </div>
  )
}
