import { Appearance } from './theme';
import { Modal } from './Modal';
import { useState } from 'react';
import { Button } from './components/controls';

import { Settings as SettingsIcon } from 'lucide-react';
import { type Market } from './api';
import { isDesktop } from './desktop';
import { DesktopCollector, DesktopConnectionSettings } from './DesktopShell';
import { DesktopUpdates } from './DesktopUpdates';
import { Diagnostics } from './Diagnostics';

export function ApplicationSettings({ market, canRunSetup = true }: { market?: Market; canRunSetup?: boolean }) {
  return <><Appearance/>
    {isDesktop() && <>{canRunSetup && <section className="settings-section"><h2>Automatic setup</h2><p>Find WoW, install or update the addon, and connect this computer to your signed-in membership.</p><Button onClick={() => window.dispatchEvent(new Event('nauctions-setup'))}>Run setup again</Button></section>}<DesktopUpdates/><DesktopConnectionSettings/><DesktopCollector market={market}/></>}
    <Diagnostics/>
  </>;
}

export function SignedOutSettings() {
 const [open, setOpen] = useState(false);
 return <><div className="signed-out-settings"><Button variant="ghost" aria-label="Settings" onClick={() => setOpen(true)}><SettingsIcon size={19}/></Button></div>{open && <Modal title="Settings" onClose={() => setOpen(false)}><ApplicationSettings canRunSetup={false}/></Modal>}</>;
}
