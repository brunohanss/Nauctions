import { Component, useState, type ErrorInfo, type ReactNode } from 'react';
import { buildInfo, diagnosticsEntries, reportFailure, setTelemetryEnabled, telemetryEnabled, track } from './telemetry';
import { desktopInvoke, isDesktop } from './desktop';

export function Diagnostics() {
  const [enabled, setEnabled] = useState(telemetryEnabled), [busy, setBusy] = useState(false), [message, setMessage] = useState('');
  async function run(exportFile: boolean) {
    setBusy(true); setMessage('');
    try {
      if (isDesktop()) {
        if (exportFile) {
          const path = await desktopInvoke<string | null>('export_diagnostics', { frontend: diagnosticsEntries() });
          setMessage(path ? `Saved diagnostics: ${path}` : 'Export cancelled.');
        } else {
          const checks = await desktopInvoke<Record<string, unknown>>('diagnostics_snapshot');
          setMessage(JSON.stringify(checks, null, 2));
        }
      } else {
        if (exportFile) {
          const blob = new Blob([JSON.stringify({ ...buildInfo, entries: diagnosticsEntries() }, null, 2)], { type: 'application/json' });
          const url = URL.createObjectURL(blob), link = document.createElement('a');
          link.href = url; link.download = 'nauctions-diagnostics.json'; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
          setMessage('Diagnostics downloaded.');
        } else {
          const response = await fetch('/healthz', { signal: AbortSignal.timeout(5000), credentials: 'omit' });
          setMessage(response.ok ? 'The server is reachable.' : `The server returned ${response.status}.`);
        }
      }
      track(exportFile ? 'diagnostics_exported' : 'setup_checked');
    } catch { setMessage(`The check failed. Reference: ${reportFailure('diagnostics_failed')}`); }
    finally { setBusy(false); }
  }
  async function toggle(value: boolean) {
    setBusy(true);
    try {
      if (isDesktop()) await desktopInvoke('set_observability', { enabled: value });
      setTelemetryEnabled(value); setEnabled(value);
    } catch { setMessage('Cannot save this preference.'); }
    finally { setBusy(false); }
  }
  return <details className="desktop-connection"><summary>Help and diagnostics</summary>
    <p>Nauctions {buildInfo.release} · Build {buildInfo.revision}</p>
    <p>Share the exported file when you report a problem. It contains setup checks and recent structured diagnostics. It excludes credentials, game files, account names, and raw collector output.</p>
    <label><input type="checkbox" checked={enabled} disabled={busy} onChange={e => void toggle(e.target.checked)}/> Send anonymous usage, error reports, and structured logs to PostHog EU</label>
    <p className="chart-note">No session recording or form capture. Development builds keep remote reports off. Local diagnostics remain available.</p>
    <div className="desktop-actions"><button disabled={busy} onClick={() => void run(false)}>Check my setup</button><button disabled={busy} onClick={() => void run(true)}>Export diagnostics</button></div>
    {message && <pre className="desktop-output" role="status">{message}</pre>}
  </details>;
}

export class RecoveryBoundary extends Component<{ children: ReactNode }, { incident: string | null }> {
  state = { incident: null as string | null };
  static getDerivedStateFromError() { return { incident: 'pending' }; }
  componentDidCatch(error: Error, _info: ErrorInfo) { this.setState({ incident: reportFailure('react_render_failed', {}, error) }); }
  render() {
    if (!this.state.incident) return this.props.children;
    return <><div className="auth"><div className="login-card"><h1>Nauctions could not show this screen</h1><p>Reference: {this.state.incident}</p><button onClick={() => location.reload()}>Reload Nauctions</button></div></div><Diagnostics/></>;
  }
}
