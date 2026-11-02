import { Progress } from './components/ui/progress';
import { Alert } from './components/ui/alert';
import { Button, Details } from './components/controls';
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
  return <Details className="settings-section" open={info?.version ? true : undefined}>
    <summary>App updates{info?.version ? ` · Version ${info.version} available` : info ? ` · Version ${info.current_version}` : ''}</summary>
    {info && !info.version && <p role="status">Nauctions is up to date.</p>}
    {info?.version && <><p>Version {info.version} is available. Installation closes Nauctions and restarts it. Your settings and queued uploads are kept.</p>{info.notes && <pre className="desktop-output">{info.notes}</pre>}<Button disabled={busy} onClick={() => void install()}>Download update and restart</Button></>}
    <Button disabled={busy} onClick={() => void check()}>{busy && !progress ? 'Checking for updates…' : 'Check for updates'}</Button>
    {progress?.total ? <Progress aria-label="Update download" value={Math.min(100, progress.downloaded / progress.total * 100)}/> : null}{progress && <p role="status">{progress.phase === 'installing' ? 'Installing update… Nauctions will restart.' : progress.total ? `Downloading update… ${Math.min(100, Math.floor(progress.downloaded / progress.total * 100))}%` : 'Downloading update…'}</p>}
    {error && <Alert role="alert" className="error-state">{error}</Alert>}
  </Details>;
}
