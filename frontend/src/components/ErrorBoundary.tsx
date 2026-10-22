import { Component, type ErrorInfo, type ReactNode } from 'react'
import { withTranslation, type WithTranslation } from 'react-i18next'

type Props = WithTranslation & { children: ReactNode; fallback?: (error: Error, reset: () => void) => ReactNode }
type State = { error: Error | null }

class ErrorBoundaryInner extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('[ErrorBoundary]', error, info.componentStack)
  }

  reset = () => this.setState({ error: null })

  render() {
    const { error } = this.state
    const { t } = this.props
    if (!error) return this.props.children
    if (this.props.fallback) return this.props.fallback(error, this.reset)
    return (
      <div className="p-6 flex flex-col gap-3 max-w-3xl">
        <h2 className="text-xl font-semibold text-red-400">{t('errors.something_wrong')}</h2>
        <pre className="text-xs font-mono text-muted-foreground whitespace-pre-wrap break-all border border-border rounded-md p-3 bg-muted/30">
          {error.message}
          {error.stack ? `\n\n${error.stack}` : ''}
        </pre>
        <button
          onClick={this.reset}
          className="self-start px-3 py-1.5 rounded-md bg-primary text-primary-foreground text-sm"
        >
          {t('errors.retry')}
        </button>
      </div>
    )
  }
}

export const ErrorBoundary = withTranslation()(ErrorBoundaryInner)
