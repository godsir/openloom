// Icon barrel — re-exports from lucide-react
import { Star } from 'lucide-react'

export {
  Search as IconSearch,
  Plus as IconPlus,
  Settings as IconSettings,
  Star as IconStar,
  ChevronRight as IconChevronRight,
  ChevronDown as IconChevronDown,
  SendHorizonal as IconSend,
  Trash2 as IconTrash,
  Copy as IconCopy,
  Cpu as IconCpu,
  X as IconX,
  X as IconWinClose,
  Wrench as IconWrench,
  Minus as IconWinMin,
  Square as IconWinMax,
  AlertCircle as IconAlertCircle,
  Check as IconCheck,
  RefreshCw as IconRefresh,
  Zap as IconZap,
  Menu as IconMenu,
  Pencil as IconEdit,
  Pin as IconPin,
  PinOff as IconPinOff,
  Command as IconCommand,
  PanelLeftClose as IconPanelLeftClose,
  PanelLeft as IconPanelLeft,
  Shield as IconShield,
  Brain as IconBrain,
  File as IconFile,
  Activity as IconActivity,
  Loader as IconLoader,
  XCircle as IconXCircle,
  Wifi as IconWifi,
  WifiOff as IconWifiOff,
  ExternalLink as IconExternalLink,
} from 'lucide-react'

export function IconStarFilled({ size = 16, className = '' }: { size?: number; className?: string }) {
  return <Star size={size} className={className} fill="currentColor" />
}
