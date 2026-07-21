import type { Platform } from '../../stores/im'
import PlatformIcon from '../shared/PlatformIcon'
import wrapperStyles from '../input/InputArea.module.css'
import styles from './ImSessionNotice.module.css'

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
    <div className={wrapperStyles.wrapper}>
      <div className={wrapperStyles.container}>
        {/* 独立 module 样式 + 入场过渡；i18n 走正确的 vars 占位（此前把字符串
            当 vars 传入，兜底逻辑实际失效） */}
        <div className={styles.notice}>
          <PlatformIcon platform={platform} size={32} />
          <div>
            <div className={styles.title}>{t('im.sessionNoticeTitle', { platform: label })}</div>
            <div className={styles.body}>{t('im.sessionNoticeBody', { platform: label })}</div>
          </div>
        </div>
      </div>
    </div>
  )
}
