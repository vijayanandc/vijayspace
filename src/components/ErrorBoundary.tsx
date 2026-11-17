// Catches render errors so one broken page shows a recoverable message instead of a blank app.

import { Component, type ErrorInfo, type ReactNode } from 'react';

import { Button } from './Button';
import { MessageBar } from './MessageBar';

interface Props {
  children: ReactNode;
  /** Clears the error when it changes (e.g. the route), so navigating away recovers. */
  resetKey?: string;
}

interface State {
  error: Error | null;
  resetKey?: string;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, resetKey: this.props.resetKey };

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    return props.resetKey !== state.resetKey ? { error: null, resetKey: props.resetKey } : null;
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('Render error', error, info.componentStack);
  }

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="page">
        <MessageBar
          intent="error"
          role="alert"
          title="Something went wrong on this page."
          actions={
            <>
              <Button size="sm" onClick={() => this.setState({ error: null })}>
                Try again
              </Button>
              <Button size="sm" onClick={() => window.location.reload()}>
                Reload
              </Button>
            </>
          }
        >
          {error.message}
        </MessageBar>
      </div>
    );
  }
}
