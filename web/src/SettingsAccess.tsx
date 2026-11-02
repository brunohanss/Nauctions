import { useRef } from 'react';
import { Settings as SettingsIcon, X } from 'lucide-react';
import { type Market } from './api';
import { isDesktop } from './desktop';
import { DesktopCollector, DesktopConnectionSettings } from './DesktopShell';
import { DesktopUpdates } from './DesktopUpdates';
import { Diagnostics } from './Diagnostics';

export function ApplicationSettings({ market, canRunSetup = true }: { market?: Market; canRunSetup?: boolean }) {
  return <>
    {isDesktop() && <>{canRunSetup && <section className="settings-section"><h2>Automatic setup</h2><p>Find WoW, install or update the addon, and connect this computer to your signed-in membership.</p><button onClick={() => window.dispatchEvent(new Event('nauctions-setup'))}>Run setup again</button></section>}<DesktopUpdates/><DesktopConnectionSettings/><DesktopCollector market={market}/></>}
    <Diagnostics/>
  </>;
}

export function SignedOutSettings() {
  const dialog = useRef<HTMLDialogElement>(null);
  return <>
    <div className="signed-out-settings"><button className="settings-button" aria-label="Settings" title="Settings" onClick={() => dialog.current?.showModal()}><SettingsIcon size={19}/></button></div>
    <dialog ref={dialog} className="settings-dialog" aria-labelledby="settings-dialog-title">
      <div className="settings-dialog-heading"><h1 id="settings-dialog-title">Settings</h1><button aria-label="Close settings" onClick={() => dialog.current?.close()}><X size={20}/></button></div>
      <ApplicationSettings canRunSetup={false}/>
    </dialog>
  </>;
}
