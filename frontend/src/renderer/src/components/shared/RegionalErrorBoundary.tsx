import { Component, type ReactNode } from 'react'

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
            <p className="text-xs text-[var(--red)] mb-2">组件加载失败</p>
            <button
              onClick={() => this.setState({ error: null })}
              className="text-xs text-[var(--text-muted)] hover:text-[var(--text-light)] underline transition-colors-fast"
            >
              重试
            </button>
          </div>
        </div>
      )
    }
    return this.props.children
  }
}
