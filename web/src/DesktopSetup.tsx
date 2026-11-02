import { Card } from './components/ui/card';
import { Spinner } from './components/ui/spinner';
import { Alert } from './components/ui/alert';
import { Button, Label, Select } from './components/controls';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Check, LoaderCircle } from 'lucide-react';
import type { Member } from './api';
import { desktopInvoke, isDesktop, type DesktopConfig } from './desktop';
import { SignedOutSettings } from './SettingsAccess';

type Phase = 'search' | 'choose' | 'addon' | 'collector' | 'ready';
const steps = ['Find your WoW installation', 'Install the Nauctions addon', 'Connect this computer'];

export function DesktopSetup({ member, children }: { member: Member; children: ReactNode }) {
  const [phase, setPhase] = useState<Phase>('search'), [roots, setRoots] = useState<string[]>([]);
  const [selected, setSelected] = useState(''), [error, setError] = useState(''), [busy, setBusy] = useState(true), [skipped, setSkipped] = useState(false);
  const pending = useRef<Promise<void> | null>(null);
  async function chooseFolder() {
    setBusy(true); setError('');
    try { const config = await desktopInvoke<DesktopConfig>('select_game_path', { executable: false }); if (config.game_root) await run(config.game_root); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  async function run(root?: string) {
    setBusy(true); setError('');
    try {
      if (!root) {
        setPhase('search');
        const found = await desktopInvoke<string[]>('discover_wow');
        setRoots(found); setSelected(found[0] ?? '');
        if (found.length !== 1) { setPhase('choose'); return; }
        root = found[0];
      }
      setSelected(root);
      setPhase('addon');
      await desktopInvoke('prepare_wow', { root });
      setPhase('collector');
      await desktopInvoke('auto_connect_collector', { csrf: member.csrf_token });
      setPhase('ready');
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  useEffect(() => {
    if (!isDesktop()) return;
    // React StrictMode repeats effects. Share the in-flight registration.
    if (!pending.current) pending.current = run();
  }, []);
  useEffect(() => {
    const restart = () => {
      if (busy) return;
      setSkipped(false);
      pending.current = run();
    };
    window.addEventListener('nauctions-setup', restart);
    return () => window.removeEventListener('nauctions-setup', restart);
  }, [busy]);
  if (!isDesktop() || phase === 'ready' || skipped) return children;
  const current = phase === 'addon' ? 1 : phase === 'collector' ? 2 : 0;
  return <><SignedOutSettings/><div className="auth"><Card className="login-card setup-card" aria-labelledby="setup-title">
    <div className="brand">Nauctions</div><h1 id="setup-title">Set up your companion</h1>
    <p>You are signed in as {member.display_name}. Nauctions will find WoW, install the addon, and connect this computer automatically.</p>
    <ol className="setup-steps" aria-label="Setup progress">{steps.map((step, index) => <li key={step} aria-current={index === current ? 'step' : undefined}>{index < current ? <Check size={18} aria-hidden="true"/> : busy && index === current ? <Spinner aria-hidden="true"/> : <span aria-hidden="true">{index + 1}</span>}{step}</li>)}</ol>
    <div role="status" aria-live="polite">{busy && (phase === 'search' ? 'Searching for WoW on this computer…' : phase === 'addon' ? 'Checking and installing the addon…' : 'Connecting the collector to your membership…')}</div>
    {phase === 'choose' && <><p>{roots.length ? 'More than one WoW installation was found. Select the one you use.' : 'WoW was not found. Open WoW and retry the search, or select its folder.'}</p>{roots.length > 0 && <Label>WoW installation<Select value={selected} disabled={busy} onChange={e => setSelected(e.target.value)}>{roots.map(root => <option key={root} value={root}>{root}</option>)}</Select></Label>}<div className="desktop-actions">{roots.length > 0 && <Button className="primary" disabled={busy} onClick={() => void run(selected)}>Use this installation</Button>}<Button disabled={busy} onClick={() => void chooseFolder()}>Choose WoW folder</Button><Button disabled={busy} onClick={() => void run()}>Search again</Button></div></>}
    {error && <Alert role="alert" className="error-state"><p>{error}</p><div className="desktop-actions"><Button disabled={busy} onClick={() => void run(selected || undefined)}>Retry setup</Button>{phase !== 'choose' && <Button disabled={busy} onClick={() => void chooseFolder()}>Choose another WoW folder</Button>}</div></Alert>}
    <p className="chart-note">Close WoW if an addon install is needed. Any existing AuctionDB addon is backed up in the WoW folder under NauctionsBackups. Saved game data stays in place.</p>
    {!busy && <Button className="text-button" onClick={() => setSkipped(true)}>Continue without collection</Button>}
  </Card></div></>;
}
