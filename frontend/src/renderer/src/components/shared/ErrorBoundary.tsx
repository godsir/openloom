import { Component, type ReactNode } from 'react'

interface Props {
  children: ReactNode
  fallback?: ReactNode
}

interface State {
  error: Error | null
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, info: { componentStack: string }) {
    console.error('[ErrorBoundary]', error.message, info.componentStack)
  }

  render() {
    if (this.state.error) {
      return (
        this.props.fallback || (
          <div className="flex items-center justify-center min-h-[200px]">
            <div className="text-center max-w-sm">
              <h3 className="text-sm font-semibold text-red-400 mb-2">
                发生错误
              </h3>
              <p className="text-xs text-zinc-500 mb-3">
                {this.state.error.message}
              </p>
              <button
                onClick={() => this.setState({ error: null })}
                className="px-3 py-1.5 bg-zinc-700 text-zinc-200 text-sm rounded-lg hover:bg-zinc-600 transition-colors"
              >
                重试
              </button>
            </div>
          </div>
        )
      )
    }

    return this.props.children
  }
}
