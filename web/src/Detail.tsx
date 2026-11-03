import { WatchlistButton } from './WatchlistState';
import { Percentage } from './Percentage';
import { ItemTooltip } from './ItemTooltip';
import { MerchantProductDetails } from './MerchantProfit';
import { ToggleGroup, ToggleGroupItem } from './components/ui/toggle-group';
import { Alert } from './components/ui/alert';
import { Button, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, Details } from './components/controls';
import { useMarketTime, useMarketRefresh, depthFresh } from './marketClock';
import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft } from "lucide-react";
import { api, price, time, type Item, type Price } from "./api";
import { DiscountBadge, FreshnessIndicator, ItemIcon, WowMoney } from "./market-ui";
import { canConnectObservations, observedPoints, differenceValue, chartProjection, numericPrice, belowReference } from "./Detail.helpers";
import { ForecastPanel } from './Forecasts';
import { type ForecastReply } from './forecast-types';
import './Detail.chart.css';

type History = {
  points: Item[];
  daily_imports: { utc_day: number; point: { min_unit_copper?: number | string; median_unit_copper?: number | string; quantity?: number | string } }[];
  seven_day?: { minimum: Price | null; maximum: Price | null; observation_count: number };
};
type Depth = {
  available: boolean;
  observed_at: string;
  listings: { stack_count: number; buyout_total_copper: string; unit_price: Price }[];
};
type Range = "24h" | "7d" | "30d" | "all";
type Series = "median" | "minimum" | "quantity";
const numberPrice = numericPrice;

export function Detail({ itemKey, market, back }: { itemKey: string; market: string; back: () => void }) {
  const now = useMarketTime(), refresh = useMarketRefresh();
  const [forecast, setForecast] = useState<ForecastReply | null>(null);
  const [range, setRange] = useState<Range>("24h");
  const [item, setItem] = useState<Item | null>(null);
  const [depth, setDepth] = useState<Depth | null>(null);
  const [history, setHistory] = useState<History | null>(null);
  const [error, setError] = useState("");
  const [chartError, setChartError] = useState("");
  const [loading, setLoading] = useState(true);
  const [retry, setRetry] = useState(0);
  const [visible, setVisible] = useState<Record<Series, boolean>>({ median: true, minimum: true, quantity: true });
  useEffect(() => {
    let active = true;
    setItem(null); setDepth(null); setError("");
    const base = `/items/${encodeURIComponent(itemKey)}`;
    const query = new URLSearchParams({ market_id: market });
    Promise.all([api<Item>(`${base}?${query}`), api<Depth>(`${base}/depth?${query}`)])
      .then(([i, d]) => { if (active) { setItem(i); setDepth(d); } })
      .catch(e => { if (active) setError(e.message); });
    return () => { active = false; };
  }, [itemKey, market, retry, refresh]);
  useEffect(() => {
    let active = true;
    setLoading(true); setChartError(""); setHistory(null);
    api<History>(`/items/${encodeURIComponent(itemKey)}/history?${new URLSearchParams({ market_id: market, range })}`)
      .then(h => { if (active) setHistory(h); })
      .catch(e => { if (active) setChartError(e.message); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [itemKey, market, range, retry, refresh]);
  const backLink = <Button className="text-button" onClick={back}><ArrowLeft size={15} /> Back to results</Button>;
  if (error) return <section className="detail-page">{backLink}<div className="empty" role="alert"><h2>Could not load this item</h2><p>{error}</p><Button onClick={() => setRetry(v => v + 1)}>Try again</Button></div></section>;
  if (!item) return <section className="detail-page">{backLink}<div className="empty" role="status">Loading item…</div></section>;
  const stale = item.freshness_status === "stale";
  return <section className="detail-page">
    <div className="detail-back-row">{backLink}<FreshnessIndicator seconds={item.freshness_seconds} stale={stale} observedAt={item.observed_at} /></div>
    <div className="detail-heading">
      <ItemTooltip name={item.name} itemId={item.item_id} itemKey={item.item_key} category={item.category} rarity={item.rarity}><div className="detail-identity"><ItemIcon name={item.name} icon={item.icon} large /><div><h1 data-item-name>{item.name}</h1><span className="detail-category">{item.category ?? "Uncategorized"}</span></div></div></ItemTooltip>
      <div className="detail-value-context"><DiscountBadge value={item.discount_percentage} /><small>{belowReference(item.min_buyout, item.baseline.price)}</small></div>
    </div>
    <WatchlistButton itemKey={itemKey}/>
    {stale && <Alert>Stale observation. The market may have changed.</Alert>}
    <dl className="detail-metrics">
      <div><dt>Current minimum</dt><dd><WowMoney value={item.min_buyout} /></dd></div>
      <div><dt>Current median</dt><dd><WowMoney value={item.median_buyout} /></dd></div>
      <div title="Robust reference calculated by the server from historical observations"><dt>7d reference</dt><dd><WowMoney value={item.baseline.price} /></dd></div>
      <div><dt>Difference</dt><dd className={(item.discount_percentage ?? 0) < 0 ? "positive" : ""}><WowMoney value={differenceValue(item.min_buyout, item.baseline.price)} /></dd></div>
      <div><dt>Observed quantity</dt><dd>{item.total_units?.toLocaleString() ?? "Unknown"}</dd></div>
      <div><dt>Last seen · Paris time</dt><dd>{time(item.observed_at)}</dd></div>
    </dl>
    <section className="chart-panel" aria-label="Observed market history and 24-hour forecast">
      <div className="detail-chart-heading"><h2>Price history and forecast</h2><span>{chartProjection(item, forecast, now) ? <>24-hour forecast change: <Percentage value={forecast!.rolling.change_percent} baseline={0} signed missing="Insufficient data" /></> : "24-hour forecast unavailable"}</span></div>
      <div className="chart-toolbar">
        <ToggleGroup type="single" value={range} onValueChange={value => {if(value) setRange(value as Range);}} className="range-tabs" aria-label="History range">{(["24h", "7d", "30d", "all"] as const).map(r => <ToggleGroupItem key={r} value={r}>{r === 'all' ? 'All' : r}</ToggleGroupItem>)}</ToggleGroup>
        <ToggleGroup type="multiple" value={(Object.keys(visible) as Series[]).filter(s=>visible[s])} onValueChange={values=>setVisible({median:values.includes('median'),minimum:values.includes('minimum'),quantity:values.includes('quantity')})} className="chart-legend" aria-label="Visible chart series">{(['median','minimum','quantity'] as const).map(s=><ToggleGroupItem value={s} key={s}><span className={`series-dot ${s}`}/>{s==='median'?'Median price':s==='minimum'?'Minimum price':'Quantity (right axis)'}</ToggleGroupItem>)}</ToggleGroup>
      </div>
      {loading ? <div className="empty chart-empty" role="status">Loading observations…</div> : chartError ? <div className="empty chart-empty" role="alert"><p>{chartError}</p><Button onClick={() => setRetry(v => v + 1)}>Try again</Button></div> : <HistoryChart item={item} forecast={forecast} asOf={now} points={history?.points ?? []} range={range} visible={visible} />}
      <p className="chart-note">Lines connect recorded prices across missing scans · Dashed line: forecast</p>
      <Details className="quiet-details"><summary>Chart details</summary><p>Times use Europe/Paris. Lines connect the available price observations, including across missing scans. Markers show recorded observations. Missing observations are not zero listings.</p>{!!history?.daily_imports.length && <p className="chart-note">{history.daily_imports.length} imported daily summaries are available. They are excluded from the scan chart because they do not represent individual observations.</p>}    {item.catalog_attribution && <p className="catalog-credit"><a href={item.catalog_attribution.url} target="_blank" rel="noreferrer">{item.catalog_attribution.text}</a></p>}
</Details>
    </section>
    <ForecastPanel itemKey={itemKey} market={market} onData={setForecast}/>
    <MerchantProductDetails itemKey={itemKey} market={market} unitPrice={item.merchant_unit_price}/>
    <div className="detail-secondary">
      <section className="listings-panel"><h2>Recent listings</h2>{!depthFresh(depth, now) || !depth ? <div className="empty"><p>Listing details require a recent, complete full scan.</p></div> : depth.listings.length ? <div className="table-scroll"><Table><TableHeader><TableRow><TableHead>Stack</TableHead><TableHead>Unit price</TableHead><TableHead>Total price</TableHead><TableHead>Observed</TableHead></TableRow></TableHeader><TableBody>{depth.listings.slice(0, 20).map((listing, i) => <TableRow key={i}><TableCell>{listing.stack_count.toLocaleString()}</TableCell><TableCell><WowMoney value={listing.unit_price} /></TableCell><TableCell><WowMoney value={{ numerator: listing.buyout_total_copper, denominator: 1 }} /></TableCell><TableCell>{time(depth.observed_at)}</TableCell></TableRow>)}</TableBody></Table>{depth.listings.length > 20 && <p className="chart-note">Showing the 20 lowest-priced stacks of {depth.listings.length.toLocaleString()} observed listings.</p>}</div> : <div className="empty"><p>No buyout listings in this full scan.</p></div>}</section>
      <Details className="summary-panel quiet-details"><summary>More price statistics</summary><dl className="summary-list">
        <div><dt>Current minimum</dt><dd><WowMoney value={item.min_buyout} /></dd></div>
        <div><dt>Current median</dt><dd><WowMoney value={item.median_buyout} /></dd></div>
        <div><dt>7d reference</dt><dd><WowMoney value={item.baseline.price} /></dd></div>
        <div><dt>7d minimum</dt><dd><WowMoney value={history?.seven_day?.minimum} /></dd></div>
        <div title="Highest observed unit buyout from full-scan listings in the last seven days"><dt>7d maximum</dt><dd><WowMoney value={history?.seven_day?.maximum} /></dd></div>
        <div><dt>Latest observed quantity</dt><dd>{item.total_units?.toLocaleString() ?? "Unknown"}</dd></div>
        <div><dt>Reference coverage</dt><dd>{item.baseline.day_count} days · {item.baseline.source_count} sources</dd></div>
      </dl></Details>
    </div>
  </section>;
}

export function HistoryChart({ points, range, visible, asOf, item, forecast }: { points: Item[]; range: Range; visible: Record<Series, boolean>; asOf: number; item?: Item; forecast?: ForecastReply | null }) {
  const [selected, setSelected] = useState<number | null>(null);
  const chartRef = useRef<SVGSVGElement>(null);
  const [size, setSize] = useState({ width: 1000, height: 340 });
  const data = useMemo(() => observedPoints(points), [points]);
  useEffect(() => setSelected(null), [points]);
  useEffect(() => {
    const element = chartRef.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry.contentRect.width > 0) setSize({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [data.length]);
  const left = size.width < 600 ? 58 : 82, right = size.width - (size.width < 600 ? 38 : 82);
  const top = 42, bottom = size.height - 45, plotWidth = right - left, plotHeight = bottom - top;
  if (!data.length) return <div className="empty chart-empty"><h3>No observations in this period</h3><p>Select a longer range or wait for the next saved scan.</p></div>;
  const projection = visible.median ? chartProjection(item, forecast, asOf) : null;
  const end = Math.max(asOf, projection?.targetAt ?? asOf), start = range === "all" ? Math.min(Date.parse(data[0].observed_at), asOf - 3600000) : asOf - ({ "24h": 1, "7d": 7, "30d": 30 }[range]) * 86400000;
  const xTime = (timestamp: number) => left + (timestamp - start) / (end - start) * plotWidth;
  const x = (p: Item) => xTime(Date.parse(p.observed_at));
  const maxPrice = Math.max(1, projection?.anchor ?? 0, projection?.target ?? 0, projection?.high ?? 0, ...data.flatMap(p => [visible.median ? numberPrice(p.median_buyout) ?? 0 : 0, visible.minimum ? numberPrice(p.min_buyout) ?? 0 : 0])) * 1.12;
  const maxQuantity = Math.max(1, ...data.map(p => p.total_units ?? 0)) * 1.12;
  const y = (value: number) => bottom - value / maxPrice * plotHeight;
  const active = selected === null ? null : data[selected];
  const choose = (clientX: number, rect: DOMRect) => {
    const px = (clientX - rect.left) / rect.width * size.width;
    if (projection && px > x(data[data.length - 1]) + 8) { setSelected(null); return; }
    let best = 0;
    for (let i = 1; i < data.length; i++) if (Math.abs(x(data[i]) - px) < Math.abs(x(data[best]) - px)) best = i;
    setSelected(best);
  };
  const labelTime = (timestamp: number) => new Date(timestamp).toLocaleString("en-GB", { timeZone: "Europe/Paris", ...(range === "24h" ? { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" } as const : { day: "numeric", month: "short" } as const) });
  return <div className="chart-container">
    <svg ref={chartRef} className="history-chart" viewBox={`0 0 ${size.width} ${size.height}`} role="img" tabIndex={0} aria-label="Observed minimum and median prices, with listed quantity on the right axis. Dashed median projection ends at the forecast target time. Use left and right arrow keys to inspect observations."
      onPointerMove={e => choose(e.clientX, e.currentTarget.getBoundingClientRect())} onPointerDown={e => choose(e.clientX, e.currentTarget.getBoundingClientRect())}
      onPointerLeave={e => { if (e.pointerType === "mouse") setSelected(null); }} onBlur={() => setSelected(null)}
      onKeyDown={e => { if (e.key === "ArrowRight" || e.key === "ArrowLeft") { e.preventDefault(); setSelected(i => Math.max(0, Math.min(data.length - 1, (i ?? 0) + (e.key === "ArrowRight" ? 1 : -1)))); } if (e.key === "Escape") setSelected(null); }}>
      <text x={left} y="19" fill="var(--chart-axis)" fontSize="11">Unit price</text><text x={right} y="19" fill="var(--chart-axis)" textAnchor="end" fontSize="11">Quantity</text>
      {[0, 1, 2, 3, 4].map(i => <g key={i}><line x1={left} x2={right} y1={top + i * plotHeight / 4} y2={top + i * plotHeight / 4} stroke="var(--chart-grid)" /><text x={left - 10} y={top + 4 + i * plotHeight / 4} textAnchor="end" fill="var(--chart-axis)" fontSize="11">{price({ numerator: Math.round(maxPrice * (1 - i / 4)).toString(), denominator: 1 })}</text><text x={right + 10} y={top + 4 + i * plotHeight / 4} fill="var(--chart-axis)" fontSize="11">{Math.round(maxQuantity * (1 - i / 4)).toLocaleString()}</text></g>)}
      {visible.quantity && data.map((p, i) => p.total_units !== null && <rect key={`q${i}`} x={x(p) - 2} y={bottom - p.total_units / maxQuantity * plotHeight} width="4" height={p.total_units / maxQuantity * plotHeight} fill="var(--chart-quantity)" opacity="0.7"><title>{time(p.observed_at)}: {p.total_units} listed units</title></rect>)}
      {(["median", "minimum"] as const).map(series => visible[series] && data.filter(p => numberPrice(p[series === "median" ? "median_buyout" : "min_buyout"]) !== null).map((p, i, seriesPoints) => {
        const key = series === "median" ? "median_buyout" : "min_buyout", value = numberPrice(p[key]);
        if (value === null) return null;
        const prior = seriesPoints[i - 1], previousValue = prior ? numberPrice(prior[key]) : null;
        const connected = previousValue !== null && canConnectObservations(prior, p, key);
        return <g key={`${series}${i}`}>{connected && <line x1={x(prior)} y1={y(previousValue!)} x2={x(p)} y2={y(value)} stroke={`var(--chart-${series})`} strokeWidth="2" />}<circle cx={x(p)} cy={y(value)} r={active === p ? 4 : 2.5} fill={`var(--chart-${series})`} stroke={active === p ? "var(--chart-highlight)" : "none"}><title>{time(p.observed_at)}: {series} {price(p[key])}</title></circle></g>;
      }))}
      {projection && <g className="chart-projection">
        <line x1={xTime(projection.anchorAt)} x2={xTime(projection.anchorAt)} y1={top} y2={bottom} stroke="var(--chart-axis)" strokeDasharray="3 5" opacity="0.5" />
        {projection.low !== null && projection.high !== null && <polygon points={`${xTime(projection.anchorAt)},${y(projection.anchor)} ${xTime(projection.targetAt)},${y(projection.high)} ${xTime(projection.targetAt)},${y(projection.low)}`} fill="var(--chart-median)" opacity="0.12"><title>Historical error range at forecast target; not a probability interval</title></polygon>}
        <line x1={xTime(projection.anchorAt)} y1={y(projection.anchor)} x2={xTime(projection.targetAt)} y2={y(projection.target)} stroke="var(--chart-median)" strokeWidth="2.5" strokeDasharray="7 5"><title>Projected median, from current observation to {time(new Date(projection.targetAt).toISOString())}. No intervening observations.</title></line>
        <circle cx={xTime(projection.targetAt)} cy={y(projection.target)} r="5" fill="var(--card)" stroke="var(--chart-median)" strokeWidth="2"><title>24-hour estimate: {price(forecast!.rolling.price)} at {time(new Date(projection.targetAt).toISOString())}</title></circle>
        <text x={right} y={top - 12} textAnchor="end" fill="var(--chart-median)" fontSize="11">24-hour estimate</text>
      </g>}
      {(size.width < 600 ? [0, 2, 4] : [0, 1, 2, 3, 4]).map(i => <text key={i} x={left + i * plotWidth / 4} y={bottom + 29} textAnchor={i === 0 ? "start" : i === 4 ? "end" : "middle"} fill="var(--chart-axis)" fontSize="11">{labelTime(start + (end - start) * i / 4)}</text>)}
      {active && <line x1={x(active)} x2={x(active)} y1="35" y2={bottom} stroke="var(--chart-crosshair)" strokeDasharray="4 4" />}
    </svg>
    {active && <div className="chart-tooltip" role="status" style={{ left: `${Math.max(2, Math.min(70, x(active) / size.width * 100))}%`, top: "12%" }}><strong>{time(active.observed_at)}</strong><span>Median: <WowMoney value={active.median_buyout} /></span><span>Minimum: <WowMoney value={active.min_buyout} /></span><span>Listed quantity: {active.total_units?.toLocaleString() ?? "Unknown"}</span><small>{active.quality} · {active.source_kind === "full_scan" ? "Full scan" : "Saved history"}</small></div>}
    {data.length < 3 && <p className="chart-note">Limited data: {data.length} observed {data.length === 1 ? "point" : "points"} in this period.</p>}
  </div>;
}

export default Detail;
