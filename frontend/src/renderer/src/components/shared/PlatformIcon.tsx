import { siWechat, siTelegram } from 'simple-icons'
import type { Platform } from '../stores/im'
import { MessageSquare as IconMessageSquare } from 'lucide-react'

// 本地资产图片路径（构建时会由 Vite 处理）
import popoPng from '../../../../asset/popo.png'
import feishuPng from '../../../../asset/feishu.png'
import dingdingPng from '../../../../asset/dingding.png'
import discordSvg from '../../../../asset/discord.svg'
import qqPng from '../../../../asset/qq.png'
import wecomPng from '../../../../asset/wecom.png'

const ASSET_IMAGES: Partial<Record<Platform, string>> = {
  popo: popoPng,
  feishu: feishuPng,
  dingtalk: dingdingPng,
  discord: discordSvg,
  qq: qqPng,
  wecom: wecomPng,
}

/** simple-icons 仍用于微信 / Telegram（没有本地图片） */
const BRAND_ICONS: Partial<Record<Platform, { path: string; hex: string }>> = {
  wechat: { path: siWechat.path, hex: `#${siWechat.hex}` },
  telegram: { path: siTelegram.path, hex: `#${siTelegram.hex}` },
}

/** 无本地图片也无 simple-icons 的平台 —— 彩色圆 + 通用 message 图标兜底 */
const FALLBACK_COLORS: Record<string, string> = {}

interface Props {
  platform: Platform
  size?: number
}

export default function PlatformIcon({ platform, size = 24 }: Props) {
  // 1. 本地图片（优先）
  const assetUrl = ASSET_IMAGES[platform]
  if (assetUrl) {
    return (
      <img
        src={assetUrl}
        alt={platform}
        width={size}
        height={size}
        style={{ borderRadius: 4, flexShrink: 0 }}
      />
    )
  }

  // 2. simple-icons SVG（微信 / Telegram）
  const brand = BRAND_ICONS[platform]
  if (brand) {
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" fill={brand.hex}>
        <path d={brand.path} />
      </svg>
    )
  }

  // 3. 兜底：彩色圆 + 通用图标
  const color = FALLBACK_COLORS[platform] ?? 'var(--text-muted)'
  return (
    <span style={{
      display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
      width: size, height: size, borderRadius: '50%', backgroundColor: color,
    }}>
      <IconMessageSquare size={Math.round(size * 0.55)} color="#fff" />
    </span>
  )
}
