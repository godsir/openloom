import { useState } from 'react'
import { useLocale } from '../../i18n'
import type { InstanceConfig } from '../../stores/im'
import styles from './ImTab.module.css'

type P = InstanceConfig['platform']

// 各平台开放平台入口 URL（不随语种变化，硬编码）
const GUIDE_URLS: Partial<Record<P, string>> = {
  telegram: 'https://t.me/BotFather',
  discord: 'https://discord.com/developers/applications',
  qq: 'https://q.qq.com',
  feishu: 'https://open.feishu.cn',
  wecom: 'https://work.weixin.qq.com',
  dingtalk: 'https://open-dev.dingtalk.com',
}

// 教程文案的 i18n key（wechat/popo 用扫码，不需要教程）
const GUIDE_KEYS: Record<P, string> = {
  wechat: '',
  popo: '',
  telegram: 'im.telegram.guide',
  discord: 'im.discord.guide',
  qq: 'im.qq.guide',
  feishu: 'im.feishu.guide',
  wecom: 'im.wecom.guide',
  dingtalk: 'im.dingtalk.guide',
}

interface Props {
  platform: P
}

/**
 * ImPlatformGuide — 可折叠的 IM 接入教程面板。
 *
 * 教程文案来自 i18n（im.<platform>.guide），多行文本按 \n 分割为步骤：
 * - 普通行：自动编号 ①②③
 * - 以 "!" 开头的行：视为"必做配置"，⚠️ 高亮显示（不编号）
 * 外链按钮跳转各平台开放平台。
 */
export default function ImPlatformGuide({ platform }: Props) {
  const { t } = useLocale()
  const [open, setOpen] = useState(false)

  const guideKey = GUIDE_KEYS[platform]
  const url = GUIDE_URLS[platform]
  if (!guideKey) return null

  const raw = t(guideKey, '')
  if (!raw) return null
  const lines = raw.split('\n').map((s) => s.trim()).filter(Boolean)

  let stepNo = 0

  return (
    <div className={styles.guideWrap}>
      <button
        type="button"
        className={styles.instanceBtn}
        onClick={(e) => { e.stopPropagation(); setOpen(!open) }}
      >
        {t('im.guideTitle', '接入教程')}
      </button>
      {open && (
        <div className={styles.guideBody} onClick={(e) => e.stopPropagation()}>
          {lines.map((line, i) => {
            const must = line.startsWith('!')
            const text = must ? line.slice(1).trim() : line
            if (!must) stepNo += 1
            return (
              <div key={i} className={must ? styles.guideMust : styles.guideStep}>
                {must ? <span>{text}</span> : <span>{stepNo}. {text}</span>}
              </div>
            )
          })}
          {url && (
            <a
              className={styles.guideLink}
              href={url}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => e.stopPropagation()}
            >
              {t('im.guideOpen', '打开开放平台')}
            </a>
          )}
        </div>
      )}
    </div>
  )
}
