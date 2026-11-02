import { Badge } from './components/ui/badge';
import { useMarketTime, observationAge } from './marketClock';
import { useState } from "react";
import { Package } from "lucide-react";
import { price, type Price } from "./api";
export function ItemIcon({ name, icon, large = false }: {
    name: string;
    icon?: string | null;
    large?: boolean;
}) { const [failed, setFailed] = useState<string | null>(null); return <span className={`item-icon${large ? ' large' : ''}`}>{icon && failed !== icon ? <img src={icon} alt={large ? name : ""} loading={large ? "eager" : "lazy"} onError={() => setFailed(icon)}/> : <Package size={large ? 28 : 16} aria-label={`${name}: icon unavailable`}/>}</span>; }
export function WowMoney({ value }: {
    value: Price | null | undefined;
}) { return <span className="money">{value && Number(value.numerator) >= 0 ? price(value) : '—'}</span>; }
export function DiscountBadge({ value, comparison = "7-day reference" }: {
    comparison?: string;
    value: number | null | undefined;
}) { return value == null ? <span className="muted">—</span> : <Badge variant="outline" className={`discount-badge ${value < 0 ? 'favorable' : value > 0 ? 'adverse' : 'neutral'}`} title={`${value < 0 ? "Below" : "Compared with"} the ${comparison}`}>{value > 0 ? '+' : ''}{value.toLocaleString('en-GB', { maximumFractionDigits: 1 })}%</Badge>; }
export function FreshnessIndicator({ seconds, stale, observedAt }: {
    seconds: number;
    stale?: boolean;
    observedAt?: string;
}) { const now = useMarketTime(); const s = observedAt ? observationAge(observedAt, now) : Math.max(0, seconds); stale = stale || s > 2700; return <span className={`freshness ${stale ? 'stale' : ''}`} title={`${observedAt ? new Date(observedAt).toLocaleString("en-GB", {timeZone:"Europe/Paris"}) + " Europe/Paris" : ''}${stale ? ' · Stale observation; the market may have changed.' : ''}`}>{s < 60 ? '<1m' : s < 3600 ? `${Math.floor(s / 60)}m` : s < 86400 ? `${Math.floor(s / 3600)}h` : `${Math.floor(s / 86400)}d`}{stale && <Badge variant="outline">Stale</Badge>}</span>; }
export function PriceSparkline({ points }: {
    points: {
        observed_at: string;
        median_buyout: Price | null;
    }[];
}) { const values = points.filter(p => p.median_buyout && Number(p.median_buyout.denominator) > 0).map(p => ({ t: Date.parse(p.observed_at), v: Number(p.median_buyout!.numerator) / Number(p.median_buyout!.denominator) })).filter(p => Number.isFinite(p.t) && Number.isFinite(p.v)).sort((a, b) => a.t - b.t); if (values.length < 2)
    return <span className="muted" title="Not enough observations yet">—</span>; const min = Math.min(...values.map(p => p.v)), max = Math.max(...values.map(p => p.v)), start = values[0].t, end = values[values.length - 1].t; let path = ''; values.forEach((p, i) => { path += `${i === 0 || p.t - values[i - 1].t > 7200000 ? 'M' : 'L'}${2 + (p.t - start) / (end - start || 1) * 68},${22 - (p.v - min) / (max - min || 1) * 20} `; }); return <svg className={`sparkline ${values.at(-1)!.v < values[0].v ? 'favorable' : values.at(-1)!.v > values[0].v ? 'adverse' : 'neutral'}`} viewBox="0 0 72 24" role="img" aria-label="Observed median prices over 24 hours"><path d={path} fill="none" stroke="currentColor" strokeWidth="1.5"/></svg>; }
