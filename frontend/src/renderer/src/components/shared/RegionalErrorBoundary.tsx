import { Component, type ReactNode } from 'react'
import { t } from '../../i18n'

interface Props { children: ReactNode; name?: string }
interface State { error: Error | null }

export default class RegionalErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State { return { error } }

  componentDidCatch(error: Error) {
    console.error(`[RegionalErrorBoundary${this.props.name ? ':' + this.props.name : ''}]`, error.message)
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex items-center justify-center py-4">
          <div className="text-center">
            <p className="text-xs text-[var(--red)] mb-2">{t('error.componentFailed')}</p>
            <button
              onClick={() => this.setState({ error: null })}
              className="text-xs text-[var(--text-muted)] hover:text-[var(--text-light)] underline transition-colors-fast"
            >
              {t('common.retry')}
            </button>
          </div>
        </div>
      )
    }
    return this.props.children
  }
}
