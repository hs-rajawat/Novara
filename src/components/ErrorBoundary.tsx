import { Component, type ErrorInfo, type ReactNode } from "react";

import { Icon } from "@/components/Icon";
import { errorMessage } from "@/lib/errors";

interface Props {
  children: ReactNode;
  /** Changing this resets the boundary — used to recover on navigation. */
  resetKey?: string;
}

interface State {
  error: unknown;
}

/**
 * Catches render-time exceptions so one broken component cannot blank the
 * entire window.
 *
 * Without this, any throw during render unmounts the whole React tree and
 * leaves an empty webview with no route back — the app looks hung rather than
 * broken, and in a packaged build there is no console to inspect.
 *
 * Recovery is deliberately in-place: `resetKey` is wired to the current route
 * in `App`, so navigating away clears the error instead of requiring a
 * restart. This must be a class component; there is no hook equivalent of
 * `componentDidCatch`.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: unknown): State {
    return { error };
  }

  componentDidUpdate(prev: Props) {
    if (this.state.error && prev.resetKey !== this.props.resetKey) {
      this.setState({ error: null });
    }
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    // Not routed through `reportError`: a toast is the wrong surface for an
    // error that has already replaced the page content, and pushing state
    // from inside this lifecycle risks a second failure while handling the
    // first.
    console.error("[render error]", error, info.componentStack);
  }

  private reset = () => this.setState({ error: null });

  render() {
    if (!this.state.error) return this.props.children;

    return (
      <div className="empty fade-up" role="alert">
        <div className="empty-icon">
          <Icon name="alert-triangle" size={22} />
        </div>
        <h3>This view could not be displayed</h3>
        <p>{errorMessage(this.state.error)}</p>
        <div className="row" style={{ gap: 8, marginTop: 14 }}>
          <button className="btn btn-primary" onClick={this.reset}>
            Try again
          </button>
          <button className="btn" onClick={() => window.location.reload()}>
            Reload NOVARA
          </button>
        </div>
      </div>
    );
  }
}
