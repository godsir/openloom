import { Component, type ReactNode } from 'react'
import { t } from '../../i18n'

interface Props { children: ReactNode; fallback?: ReactNode }
interface State { error: Error | null }

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State { return { error } }

  componentDidCatch(error: Error, info: { componentStack: string }) {
    console.error('[ErrorBoundary]', error.message, info.componentStack)
  }

  render() {
    if (this.state.error) {
      return (
        this.props.fallback || (
          <div className="flex items-center justify-center min-h-[200px]">
            <div className="text-center max-w-sm">
              <h3 className="text-sm font-semibold text-[var(--red)] mb-2">{t('error.errorOccurred')}</h3>
              <p className="text-xs text-[var(--text-muted)] mb-4">
                {this.state.error.message}
              </p>
              <button
                onClick={() => this.setState({ error: null })}
                className="px-4 py-1.5 rounded-[var(--r-sm)] bg-[var(--bg-card)] text-[var(--text-light)] hover:bg-[rgba(255,255,255,0.04)] border border-[var(--border)] text-sm transition-colors-fast"
              >
                {t('common.retry')}
              </button>
            </div>
          </div>
        )
      )
    }
    return this.props.children
  }
}
