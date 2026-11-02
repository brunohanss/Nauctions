import { useEffect, useState, type ReactNode } from 'react';

// Mount the app immediately so connection checks run during the splash.
export function StartupGate({ children }: { children: ReactNode }) {
  const [elapsed, setElapsed] = useState(false);
  useEffect(() => {
    const timer = window.setTimeout(() => setElapsed(true), 2_000);
    return () => window.clearTimeout(timer);
  }, []);
  return <>
    {!elapsed && <StartupScreen/>}
    <div className="startup-content" hidden={!elapsed}>{children}</div>
  </>;
}

export function StartupScreen() {
  return <div className="startup-screen" role="status" aria-label="Loading Nauctions" aria-live="polite">
    <div className="startup-identity" aria-hidden="true">
      <span className="startup-mark"><i/><i/><i/></span>
      <div><div className="startup-name">Nauctions</div><p className="startup-caption">Loading your market…</p></div>
    </div>
  </div>;
}
