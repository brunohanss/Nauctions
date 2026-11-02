import { ToggleGroup, ToggleGroupItem } from './components/ui/toggle-group';
import { Empty } from './components/ui/empty';
import { Button, Label, Select } from './components/controls';
import { useEffect, useRef, useState } from 'react';
import { api, price, time, type Item, type Price } from './api';
import { WowMoney } from './market-ui';

export type HistoricalPair = { entry_at: number; exit_at: number; entry_price: Price; exit_price: Price; later_check: boolean };
const value = (p: Price | null) => p ? Number(p.numerator) / Number(p.denominator) : null;
const HOUR = 3600000;

export function StrategyChart({ itemKey, market, asOf, pairs, entry, exit }: {
  itemKey: string; market: string; asOf: string; pairs: HistoricalPair[]; entry: Price | null; exit: Price | null;
}) {
  const [points, setPoints] = useState<Item[]>([]), [loading, setLoading] = useState(true), [error, setError] = useState('');
  const [retry, setRetry] = useState(0), [range, setRange] = useState(pairs.length ? 'example' : '7d'), [pairIndex, setPairIndex] = useState(0);
  const [active, setActive] = useState<number | null>(null), [width, setWidth] = useState(900);
  const host = useRef<HTMLDivElement>(null);
  const examples = [...pairs].sort((a, b) => b.entry_at - a.entry_at);
  const pair = examples[pairIndex];
  useEffect(() => {
    let live = true; setLoading(true); setError('');
    api<{ points: Item[] }>(`/items/${encodeURIComponent(itemKey)}/history?${new URLSearchParams({ market_id: market, range: 'all' })}`)
      .then(d => { if (live) setPoints(d.points.filter(p => p.quality === 'complete' && p.presence !== 'absent_in_complete_scan' && value(p.median_buyout) != null)); })
      .catch(e => { if (live) setError(e.message); }).finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, [itemKey, market, retry]);
  useEffect(() => {
    if (!host.current) return;
    const observer = new ResizeObserver(([e]) => setWidth(Math.max(300, e.contentRect.width)));
    observer.observe(host.current); return () => observer.disconnect();
  }, []);
  useEffect(() => setActive(null), [range, pairIndex]);
  const now = Date.parse(asOf), padding = pair ? Math.max(4 * HOUR, (pair.exit_at - pair.entry_at) * 1000 * .3) : HOUR;
  const end = range === 'example' && pair ? Math.min(now, pair.exit_at * 1000 + padding) : now;
  const start = range === 'example' && pair ? pair.entry_at * 1000 - padding : range === 'all' ? Math.min(...points.map(p => Date.parse(p.observed_at)), now - HOUR) : now - (range === '30d' ? 30 : 7) * 24 * HOUR;
  const data = points.filter(p => Date.parse(p.observed_at) >= start && Date.parse(p.observed_at) <= end).sort((a, b) => Date.parse(a.observed_at) - Date.parse(b.observed_at));
  const markers = pair ? [{ name: 'ENTER', at: pair.entry_at, p: pair.entry_price, color: 'var(--success)' }, { name: 'EXIT', at: pair.exit_at, p: pair.exit_price, color: 'var(--warning)' }] : [];
  const prices = [...data.map(p => value(p.median_buyout)!), ...[entry, exit].map(value).filter((v): v is number => v != null), ...markers.map(m => value(m.p)!)];
  const low = Math.min(...prices, Infinity), high = Math.max(...prices, 1), spread = Math.max(high - low, high * .1);
  const min = Math.max(0, low - spread * .15), max = high + spread * .2;
  const left = width < 500 ? 65 : 85, right = width - 20, top = 68, bottom = 275;
  const x = (at: number) => left + (at - start) / Math.max(1, end - start) * (right - left);
  const y = (v: number) => bottom - (v - min) / Math.max(1, max - min) * (bottom - top);
  const point = active == null ? null : data[active];
  const labelTime = (at: number) => new Date(at).toLocaleString('en-GB', { timeZone: 'Europe/Paris', day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit' });
  return <section className="strategy-history" aria-label="Strategy price history">
    <div className="strategy-chart-heading"><div><h3>Historical price</h3><p>Actual asking prices · Paris time</p></div>
      <ToggleGroup type="single" value={range} onValueChange={v=>{if(v)setRange(v)}} className="range-tabs" aria-label="Chart period">{[['example', 'Last pattern'], ['7d', '7 days'], ['30d', '30 days'], ['all', 'All history']].map(([key, label]) => <ToggleGroupItem value={key} key={key} disabled={key === 'example' && !pair}>{label}</ToggleGroupItem>)}</ToggleGroup>
    </div>
    <p className="strategy-chart-legend"><span>Solid: observed median</span>{entry && <span className="enter-text">Green dashed: entry reference</span>}{exit && <span className="exit-text">Amber dashed: exit target</span>}</p>
    <div ref={host} className="strategy-chart-host">
      {loading ? <p role="status">Loading price history…</p> : error ? <p role="alert">{error} <Button onClick={() => setRetry(n => n + 1)}>Retry history</Button></p> : !data.length ? <Empty className="empty-state">No observed prices in this period.</Empty> : <>
        <svg viewBox={`0 0 ${width} 320`} role="img" tabIndex={0} aria-label="Historical median asking prices. ENTER and EXIT mark actual scans from the selected historical example. Use arrow keys to inspect prices."
          onPointerLeave={() => setActive(null)} onPointerMove={e => {
            const rect = e.currentTarget.getBoundingClientRect(), at = start + ((e.clientX - rect.left) / rect.width * width - left) / (right - left) * (end - start);
            let closest = 0; for (let i = 1; i < data.length; i++) if (Math.abs(Date.parse(data[i].observed_at) - at) < Math.abs(Date.parse(data[closest].observed_at) - at)) closest = i;
            setActive(closest);
          }} onKeyDown={e => { if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') { e.preventDefault(); setActive(n => Math.max(0, Math.min(data.length - 1, (n ?? 0) + (e.key === 'ArrowRight' ? 1 : -1)))); } }}>
          {[0, 1, 2, 3].map(i => { const v = min + (max - min) * i / 3; return <g key={i}><line x1={left} x2={right} y1={y(v)} y2={y(v)} stroke="var(--chart-grid)"/><text x={left - 10} y={y(v) + 4} textAnchor="end" fill="var(--chart-axis)" fontSize="11">{price({ numerator: Math.round(v), denominator: 1 })}</text></g>; })}
          {[[entry, 'var(--success)'], [exit, 'var(--warning)']].map(([p, color], i) => p && <line key={i} x1={left} x2={right} y1={y(value(p as Price)!)} y2={y(value(p as Price)!)} stroke={color as string} strokeDasharray="5 5" opacity=".5"/>)}
          {data.map((p, i) => { const prior = data[i - 1], at = Date.parse(p.observed_at), v = value(p.median_buyout)!; return <g key={`${p.observed_at}:${i}`}>
            {prior && at - Date.parse(prior.observed_at) <= 6 * HOUR && <line x1={x(Date.parse(prior.observed_at))} x2={x(at)} y1={y(value(prior.median_buyout)!)} y2={y(v)} stroke="var(--chart-median)" strokeWidth="2"/>}
            <circle cx={x(at)} cy={y(v)} r={active === i ? 5 : 2.5} fill="var(--chart-median)"><title>{time(p.observed_at)} · {price(p.median_buyout)}</title></circle>
          </g>; })}
          {markers.filter(m => m.at * 1000 >= start && m.at * 1000 <= end).map((m, i) => { const cx = x(m.at * 1000), cy = y(value(m.p)!); return <g key={m.name}>
            <line x1={cx} x2={cx} y1={i === 0 ? 31 : 53} y2={bottom} stroke={m.color} strokeDasharray="3 4" opacity=".7"/>
            <circle cx={cx} cy={cy} r="7" fill={m.color} stroke="var(--card)" strokeWidth="3"/>
            <rect x={Math.max(left, Math.min(right - 68, cx - 34))} y={i === 0 ? 8 : 30} width="68" height="23" rx="5" fill={m.color}/>
            <text x={Math.max(left, Math.min(right - 68, cx - 34)) + 34} y={i === 0 ? 24 : 46} textAnchor="middle" fill="var(--background)" fontSize="11" fontWeight="800">{m.name}</text>
          </g>; })}
          {[0, .5, 1].map(n => <text key={n} x={left + n * (right - left)} y="304" textAnchor={n === 0 ? 'start' : n === 1 ? 'end' : 'middle'} fill="var(--chart-axis)" fontSize="11">{labelTime(start + n * (end - start))}</text>)}
        </svg>
        <div className="strategy-chart-readout" aria-live="polite">{point ? <>{time(point.observed_at)} · <WowMoney value={point.median_buyout}/> per unit</> : 'Hover over the chart, or use arrow keys, to inspect prices. Gaps mean missing scans.'}</div>
      </>}
    </div>
    {pair && <div className="strategy-example"><div><Label htmlFor="strategy-example">Historical example</Label><Select id="strategy-example" value={pairIndex} onChange={e => { setPairIndex(Number(e.target.value)); setRange('example'); }}>{examples.map((p, i) => <option key={p.entry_at} value={i}>{labelTime(p.entry_at * 1000)} · {p.later_check ? 'Later check' : 'Training'}</option>)}</Select></div>
      <p><strong className="enter-text">ENTER</strong> {time(new Date(pair.entry_at * 1000).toISOString())} at <WowMoney value={pair.entry_price}/><span aria-hidden="true"> → </span><strong className="exit-text">EXIT</strong> {time(new Date(pair.exit_at * 1000).toISOString())} at <WowMoney value={pair.exit_price}/></p>
      <small>Observed price comparison, not a completed trade. Examples include losses.</small>
    </div>}
  </section>;
}
