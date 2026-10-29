import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
const Clock = createContext<string | null>(null);
export function MarketClock({ asOf, children }: { asOf?: string | null; children: ReactNode }) { return <Clock.Provider value={asOf ?? null}>{children}</Clock.Provider>; }
export function useMarketTime() {
  const replay = useContext(Clock);
  const [now, setNow] = useState(Date.now());
  useEffect(() => { if (replay) return; const tick = () => setNow(Date.now()); const timer = setInterval(tick, 1000); window.addEventListener('focus', tick); return () => { clearInterval(timer); window.removeEventListener('focus', tick); }; }, [replay]);
  return replay ? Date.parse(replay) : now;
}
export function useMarketRefresh() {
  const [tick, setTick] = useState(0);
  useEffect(() => { const refresh = () => setTick(n => n + 1); const timer = setInterval(refresh, 30_000); window.addEventListener('focus', refresh); return () => { clearInterval(timer); window.removeEventListener('focus', refresh); }; }, []);
  return tick;
}
export const observationAge = (observedAt: string, now: number) => Math.max(0, (now - Date.parse(observedAt)) / 1000);
export const depthFresh = (depth: { available: boolean; observed_at: string } | null, now: number) => !!depth?.available && observationAge(depth.observed_at, now) <= 2700;
