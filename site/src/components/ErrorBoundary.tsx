import { Component, type ReactNode } from 'react'

interface State {
  error?: Error
  key?: string
}

/** Keeps one broken view from blanking the whole dashboard; resets when `resetKey` changes. */
export class ErrorBoundary extends Component<{ children: ReactNode; resetKey?: string }, State> {
  state: State = {}
  static getDerivedStateFromError(error: Error) {
    return { error }
  }
  static getDerivedStateFromProps(p: { resetKey?: string }, s: State) {
    if (p.resetKey !== s.key) return { error: undefined, key: p.resetKey }
    return null
  }
  render() {
    if (this.state.error)
      return (
        <div className="empty-note">
          <b>This view failed to render.</b>
          <pre className="json">{String(this.state.error.stack ?? this.state.error)}</pre>
        </div>
      )
    return this.props.children
  }
}
