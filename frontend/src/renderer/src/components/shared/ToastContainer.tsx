import { useStore } from '../../stores'
import type { Toast } from '../../stores/toast'
import { IconX } from '../../utils/icons'

const TYPE_STYLES: Record<string, string> = {
  info: 'border-[rgba(var(--accent-rgb),.15)] bg-[var(--accent-light)] text-[var(--accent)]',
  success: 'border-[var(--green)]/15 bg-[var(--green-light)] text-[var(--green)]',
  warning: 'border-[var(--amber)]/15 bg-[var(--amber-light)] text-[var(--amber)]',
  error: 'border-[var(--red)]/15 bg-[var(--red-light)] text-[var(--red)]',
}

export default function ToastContainer() {
  const toasts = useStore((s: any) => s.toasts as Toast[])
  const removeToast = useStore((s: any) => s.removeToast as (id: string) => void)

  if (toasts.length === 0) return null

  return (
    <div className="fixed bottom-16 right-4 z-[200] flex flex-col gap-2 max-w-sm">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`flex items-center gap-2 px-3.5 py-2.5 rounded-[var(--r-sm)] border text-sm shadow-lg backdrop-blur-md toast-enter ${TYPE_STYLES[t.type]}`}
        >
          <span className="flex-1 text-[13px]">{t.message}</span>
          {t.action && (
            <button
              onClick={() => {
                t.action!.onClick()
                removeToast(t.id)
              }}
              className="shrink-0 px-2 py-0.5 rounded-[var(--r-sm)] bg-white/10 hover:bg-white/20 text-xs font-medium transition-colors-fast"
            >
              {t.action.label}
            </button>
          )}
          <button
            onClick={() => removeToast(t.id)}
            className="shrink-0 opacity-50 hover:opacity-100 transition-opacity-fast"
          >
            <IconX size={14} />
          </button>
        </div>
      ))}
    </div>
  )
}
