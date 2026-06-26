import { siWechat, siTelegram, siDiscord } from 'simple-icons'
import type { Platform } from '../stores/im'
import { MessageSquare as IconMessageSquare } from 'lucide-react'

/** simple-icons does not export a dedicated qq icon, and dingtalk/feishu/wecom/popo
 *  are not in the installed version. We fall back to a coloured circle + generic
 *  message icon for those. */
const BRAND_ICONS: Partial<Record<Platform, { path: string; hex: string }>> = {
  wechat: { path: siWechat.path, hex: `#${siWechat.hex}` },
  telegram: { path: siTelegram.path, hex: `#${siTelegram.hex}` },
  discord: { path: siDiscord.path, hex: `#${siDiscord.hex}` },
}

/** Platforms not covered by simple-icons — fallback to a colour-coded circle. */
const FALLBACK_COLORS: Record<string, string> = {
  dingtalk: '#0089FF',
  feishu: '#3370FF',
  wecom: '#07C160',
  popo: '#E54D42',
  qq: '#12B7F5',
}

interface Props {
  platform: Platform
  size?: number
}

export default function PlatformIcon({ platform, size = 24 }: Props) {
  const brand = BRAND_ICONS[platform]
  if (brand) {
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" fill={brand.hex}>
        <path d={brand.path} />
      </svg>
    )
  }
  // Fallback: coloured circle + generic MessageSquare icon
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
