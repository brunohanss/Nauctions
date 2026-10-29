import { useEffect, useState, type ReactNode } from 'react';
import { api, type Market } from './api';
import { desktopInvoke, isDesktop, type DesktopConfig, type CollectorStatus } from './desktop';
import { DesktopUpdates } from './DesktopUpdates';

export function DesktopRoot({ children }: { children: ReactNode }) {
  const [config, setConfig] = useState<DesktopConfig | null>(null), [address, setAddress] = useState(''), [error, setError] = useState(''), [busy, setBusy] = useState(false), [retry, setRetry] = useState(0);
  const [connectionId, setConnectionId] = useState(0);
  useEffect(() => { if (isDesktop()) desktopInvoke<DesktopConfig>('desktop_config').then(c => { setConfig(c); setAddress(c.server ?? ''); }).catch(e => setError(String(e))); }, [retry]);
  if (!isDesktop()) return children;
  async function connect() {
    setBusy(true); setError('');
    try { const c = await desktopInvoke<DesktopConfig>('configure_server', { server: address }); setConfig(c); setAddress(c.server ?? ''); setConnectionId(n => n + 1); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  const form = <form className="desktop-connection-form" onSubmit={e => { e.preventDefault(); void connect(); }}><label>Server address<input type="url" value={address} onChange={e => setAddress(e.target.value)} placeholder="https://your-nauctions-server.example" required disabled={busy}/></label><button className="primary" disabled={busy}>{busy ? 'Connecting…' : 'Connect'}</button></form>;
  if (!config) return <div className="auth"><div role={error ? 'alert' : 'status'}>{error || 'Starting Nauctions…'}{error && <button onClick={() => { setError(''); setRetry(n => n + 1); }}>Retry</button>}</div></div>;
  if (!config.server) return <><DesktopUpdates/><div className="auth"><div className="login-card"><div className="brand">Nauctions</div><h1>Connect to your market</h1><p>Enter your shared server address. Then join with your nickname and invitation code.</p>{form}{error && <p role="alert" className="error-state">{error}</p>}</div></div></>;
  return <><DesktopUpdates/><details className="desktop-connection"><summary>Nauctions desktop · {config.server}</summary>{form}<p className="chart-note">Changing the server pauses collection and restores any membership saved for that server. Close the window to keep collection in the tray. Use Quit Nauctions in the tray to exit.</p>{error && <p role="alert" className="error-state">{error}</p>}</details><div key={`${config.server}:${connectionId}`}>{children}</div></>;
}

export function DesktopCollector({ market }: { market?: Market }) {
  const [config, setConfig] = useState<DesktopConfig | null>(null), [status, setStatus] = useState<CollectorStatus | null>(null), [busy, setBusy] = useState(false), [error, setError] = useState(''), [output, setOutput] = useState('');
  async function refresh() {
    const [c, s] = await Promise.all([desktopInvoke<DesktopConfig>('desktop_config'), desktopInvoke<CollectorStatus>('collector_state')]); setConfig(c); setStatus(s);
  }
  useEffect(() => { if (!isDesktop()) return; void refresh().catch(e => setError(String(e))); const timer = setInterval(() => { if (!busy) void refresh().catch(e => setError(String(e))); }, 10000); return () => clearInterval(timer); }, [busy]);
  if (!isDesktop()) return null;
  async function act(action: string) {
    setBusy(true); setError(''); setOutput('');
    try {
      if (action === 'folder' || action === 'executable') { setConfig(await desktopInvoke<DesktopConfig>('select_game_path', { executable: action === 'executable' })); }
      else {
        let code: string | null = null;
        if (action === 'pair') code = (await api<{ code: string }>('/devices/pair-code', 'POST', {})).code;
        setOutput(await desktopInvoke<string>('collector_action', { action, code, realm: market?.realm ?? null, house: market?.house ?? null }));
      }
      await refresh();
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  async function preferences(launch: boolean, automatic: boolean) {
    setBusy(true); setError('');
    try { setConfig(await desktopInvoke<DesktopConfig>('set_desktop_preferences', { launchAtLogin: launch, automaticCollection: automatic })); await refresh(); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  return <section className="settings-section desktop-collector"><h2>This computer</h2><p className="chart-note">Read saved AHDB scans and send them to your shared market.</p>
    <div className="settings-row"><label htmlFor="desktop-startup">Start with Windows<small>Start quietly in the tray when you sign in to Windows.</small></label><input id="desktop-startup" type="checkbox" checked={status?.startup_enabled ?? config?.launch_at_login ?? false} disabled={busy || !config} onChange={e => void preferences(e.target.checked, config!.automatic_collection)}/></div>
    <div className="settings-row"><label htmlFor="desktop-automatic">Automatic collection<small>Read AHDB saves while WoW is open, then collect its final save before standby.</small></label><input id="desktop-automatic" type="checkbox" checked={config?.automatic_collection ?? false} disabled={busy || !config} onChange={e => void preferences(status?.startup_enabled ?? config!.launch_at_login, e.target.checked)}/></div>
    {status?.startup_error && <p className="error-state" role="alert">{status.startup_error}</p>}<div className="settings-row"><span>WoW folder<small>{config?.game_root ?? 'Not selected'}</small></span><button disabled={busy} onClick={() => void act('folder')}>Choose folder</button></div>
    <div className="settings-row"><span>Collector<small>{status?.running ? status.capture_mode === 'standby' ? 'Standby · pending uploads can continue' : status.capture_mode === 'final_save' ? 'Collecting the final save' : status.capture_mode === 'playing' ? 'Collecting saved AHDB data' : status.capture_mode === 'detection_unknown' ? 'Game detection unavailable · collecting cautiously' : 'Starting automatic collection' : status?.paired ? 'Paired · stopped' : 'Not paired'}{status?.last_exit ? ` · ${status.last_exit}` : ''}</small><small>{!config?.game_root ? 'Select your WoW installation for game detection.' : status?.game_state === 'running' ? 'WoW is open' : status?.game_state === 'closed' ? 'WoW is closed' : 'WoW state is unknown'}</small></span><div className="desktop-actions"><button disabled={busy || !config?.game_root || market?.demo} onClick={() => void act('pair')}>{status?.paired ? 'Pair again' : 'Pair this computer'}</button><button disabled={busy || !status?.paired} onClick={() => void act(config?.automatic_collection ? 'stop' : 'start')}>{config?.automatic_collection ? 'Pause collection' : 'Resume collection'}</button></div></div>
    <div className="desktop-actions"><button disabled={busy || !status?.paired} onClick={() => void act('status')}>Check upload status</button><button disabled={busy || !status?.paired} onClick={() => void act('retry')}>Retry pending uploads</button></div>
    {market?.demo && <p className="chart-note">The simulated market cannot receive collector data.</p>}
    <details><summary>Preview shared AHDB history</summary><p className="chart-note">Close every WoW client first. Preview checks changes without writing to the game folder. Shared-history writes are disabled until recovery and game compatibility checks pass.</p><div className="settings-row"><span>Game executable<small>{config?.game_exe ?? 'Not selected'}</small></span><button disabled={busy || !config?.game_root} onClick={() => void act('executable')}>Choose executable</button></div><div className="desktop-actions"><button disabled={busy || !status?.paired || !market || market.demo} onClick={() => void act('sync_preview')}>Preview sync</button></div></details>
    {busy && <p role="status" className="chart-note">Working…</p>}{error && <p role="alert" className="error-state">{error}</p>}{output && <pre className="desktop-output" role="status">{output}</pre>}
  </section>;
}
