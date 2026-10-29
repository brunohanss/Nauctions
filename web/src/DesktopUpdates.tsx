import { useEffect, useState } from 'react';
import { Channel } from '@tauri-apps/api/core';
import { desktopInvoke } from './desktop';

export type UpdateInfo = { current_version: string; version: string | null; notes: string | null };
export type UpdateProgress = { phase: 'downloading' | 'installing'; downloaded: number; total: number | null };

export function DesktopUpdates() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  async function check() {
    setBusy(true); setError(''); setInfo(null); setProgress(null);
    try { setInfo(await desktopInvoke<UpdateInfo>('check_update')); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  useEffect(() => { void check(); }, []);
  async function install() {
    setBusy(true); setError(''); setProgress({ phase: 'downloading', downloaded: 0, total: null });
    const channel = new Channel<UpdateProgress>();
    channel.onmessage = setProgress;
    try { await desktopInvoke('install_update', { progress: channel }); }
    catch (e) { setError(String(e)); setProgress(null); }
    finally { setBusy(false); }
  }
  return <details className="desktop-connection" open={info?.version ? true : undefined}>
    <summary>App updates{info?.version ? ` · Version ${info.version} available` : info ? ` · Version ${info.current_version}` : ''}</summary>
    {info && !info.version && <p role="status">Nauctions is up to date.</p>}
    {info?.version && <><p>Version {info.version} is available. Installation closes Nauctions and restarts it. Your settings and queued uploads are kept.</p>{info.notes && <pre className="desktop-output">{info.notes}</pre>}<button disabled={busy} onClick={() => void install()}>Download update and restart</button></>}
    <button disabled={busy} onClick={() => void check()}>{busy && !progress ? 'Checking for updates…' : 'Check for updates'}</button>
    {progress && <p role="status">{progress.phase === 'installing' ? 'Installing update… Nauctions will restart.' : progress.total ? `Downloading update… ${Math.min(100, Math.floor(progress.downloaded / progress.total * 100))}%` : 'Downloading update…'}</p>}
    {error && <p role="alert" className="error-state">{error}</p>}
  </details>;
}
