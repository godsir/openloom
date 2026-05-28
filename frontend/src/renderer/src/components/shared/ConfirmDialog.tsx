import { IconAlertCircle } from '../../utils/icons'

interface ConfirmDialogProps {
  open: boolean
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  danger?: boolean
  onConfirm: () => void
  onCancel: () => void
}

export default function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = '确定',
  cancelLabel = '取消',
  danger,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  if (!open) return null

  return (
    <div className="fixed inset-0 z-[110] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={onCancel} />
      <div className="relative bg-[var(--bg)] border border-[var(--border-accent)] rounded-[var(--r-xl)] shadow-[var(--shadow-lg)] w-full max-w-[360px] mx-4 overflow-hidden animate-fade-in">
        <div className="p-5 flex flex-col items-center text-center gap-3">
          <div className={`w-10 h-10 rounded-full flex items-center justify-center ${danger ? 'bg-[var(--red-light)]' : 'bg-[var(--accent-subtle)]'}`}>
            <IconAlertCircle size={20} className={danger ? 'text-[var(--red)]' : 'text-[var(--accent)]'} />
          </div>
          <div>
            <h3 className="text-sm font-semibold text-[var(--text)]">{title}</h3>
            <p className="text-xs text-[var(--text-secondary)] mt-1">{message}</p>
          </div>
          <div className="flex gap-2 w-full mt-1">
            <button
              onClick={onCancel}
              className="flex-1 px-3 py-2 text-xs font-medium rounded-[var(--r-md)] bg-[var(--bg-card)] text-[var(--text-secondary)] hover:bg-[rgba(255,255,255,0.04)] border border-[var(--border)] transition-colors"
            >
              {cancelLabel}
            </button>
            <button
              onClick={onConfirm}
              className={`flex-1 px-3 py-2 text-xs font-medium rounded-[var(--r-md)] transition-colors ${
                danger
                  ? 'bg-[var(--red)] text-white hover:bg-[#dc2626]'
                  : 'bg-[var(--accent)] text-black hover:bg-[var(--accent-hover)]'
              }`}
            >
              {confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
