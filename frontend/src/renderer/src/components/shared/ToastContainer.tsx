import { useStore } from '../../stores'
import type { Toast } from '../../stores/toast'

const TYPE_STYLES: Record<string, string> = {
  info: 'border-blue-700/50 bg-blue-900/20 text-blue-300',
  success: 'border-green-700/50 bg-green-900/20 text-green-300',
  warning: 'border-yellow-700/50 bg-yellow-900/20 text-yellow-300',
  error: 'border-red-700/50 bg-red-900/20 text-red-300',
}

export default function ToastContainer() {
  const toasts = useStore((s: any) => s.toasts as Toast[])
  const removeToast = useStore((s: any) => s.removeToast as (id: string) => void)

  if (toasts.length === 0) return null

  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-sm">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`flex items-center gap-2 px-3 py-2 rounded-lg border text-sm shadow-lg backdrop-blur-sm ${TYPE_STYLES[t.type]}`}
        >
          <span className="flex-1">{t.message}</span>
          {t.action && (
            <button
              onClick={() => {
                t.action!.onClick()
                removeToast(t.id)
              }}
              className="shrink-0 px-2 py-0.5 rounded bg-white/10 hover:bg-white/20 text-xs font-medium transition-colors"
            >
              {t.action.label}
            </button>
          )}
          <button
            onClick={() => removeToast(t.id)}
            className="shrink-0 text-current opacity-50 hover:opacity-100 text-base leading-none"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  )
}
