import { useEffect, useState } from 'react';
import { api, time, type Market } from './api';
import { WowMoney } from './market-ui';
import { useMarketRefresh, useMarketTime } from './marketClock';
import { forecastFresh, percent, type ForecastPage, type ForecastReply } from './forecast-types';
import { downloadForecasts } from './forecast-export';

const methodLabel = (estimate: ForecastReply['rolling']) => estimate.range_kind.startsWith('limited_history') ? estimate.method === 'damped_daily_change' ? 'Provisional trend · limited history' : 'Basic estimate · limited history' : estimate.status === 'baseline' ? 'Unchanged-price baseline' : estimate.status === 'experimental' ? 'Experimental forecast' : 'Insufficient data';
const usable = (data: ForecastReply, now: number) => forecastFresh(data, now) && ['baseline', 'experimental'].includes(data.rolling.status) && data.rolling.price !== null;

export function ForecastEvidence({ data, now }: { data: ForecastReply; now: number }) {
  const estimate = data.rolling;
  const expired = !forecastFresh(data, now);
  return <><div className="forecast-grid">
    <article className="forecast-card"><h3>Current median</h3><strong><WowMoney value={expired ? null : data.current_median}/></strong></article>
    <article className="forecast-card"><h3>In 24 hours</h3><strong>{expired ? 'Estimate expired' : usable(data, now) ? <WowMoney value={estimate.price}/> : 'Insufficient data'}</strong>{usable(data, now) && <p>{percent(estimate.change_percent)} · {estimate.direction}</p>}<small>{methodLabel(estimate)}</small></article>
    <article className="forecast-card"><h3>Historical error range</h3><strong>{usable(data, now) ? estimate.low && estimate.high ? <><WowMoney value={estimate.low}/> – <WowMoney value={estimate.high}/></> : 'Not enough comparisons' : '—'}</strong></article>
  </div>
    {data.rise_frequencies?.map(frequency => <p className="chart-note" key={frequency.horizon_hours}><strong>Historical rises ({frequency.horizon_hours}h): </strong>{expired ? 'Estimate expired' : frequency.percent === null ? 'Insufficient comparisons' : `${frequency.percent.toFixed(1)}% · ${frequency.rises}/${frequency.comparisons}`}</p>)}
    <p className="chart-note">Historical frequency, not a probability.</p>
    <details><summary>How this is calculated</summary>
      <p>{estimate.reason}</p>
      <p className="chart-note">{estimate.observations} observed hours · {estimate.validation_pairs} historical comparisons</p>
      <p className="chart-note">Prices use fresh complete scans, in copper per unit. The historical error range is not a probability interval. Asking prices do not prove a sale.</p>
      {data.rise_frequencies?.map(frequency => <p className="chart-note" key={frequency.horizon_hours}>{frequency.horizon_hours}h: {frequency.reason}</p>)}
      {estimate.mean_absolute_log_error !== null && <p className="chart-note">{estimate.range_kind.startsWith('limited_history') ? 'Historical' : 'Later-period'} mean absolute log error: {estimate.mean_absolute_log_error.toFixed(3)}. Unchanged-price baseline: {estimate.baseline_error?.toFixed(3)}.</p>}
    </details>
  </>;
}

function ExportButton({ market, keys, disabled = false, label }: { market: string; keys: string[]; disabled?: boolean; label: string }) {
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  return <><button disabled={disabled || busy || !keys.length} onClick={async () => {
    setBusy(true); setError('');
    try { await downloadForecasts(market, keys); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  }}>{busy ? 'Preparing addon cache…' : label}</button>{error && <p role="alert">{error}</p>}</>;
}
const installNote = 'Close WoW. Extract the ZIP into Interface/AddOns. Enable Nauctions Forecast Cache, then start WoW. Each export replaces the previous cache and expires within 45 minutes.';

export function ForecastPanel({ itemKey, market, onData }: { itemKey: string; market: string; onData?: (data: ForecastReply | null) => void }) {
  const [data, setData] = useState<ForecastReply | null>(null), [error, setError] = useState(''), [retry, setRetry] = useState(0);
  const now = useMarketTime(), refresh = useMarketRefresh();
  useEffect(() => {
    let active = true; setData(null); onData?.(null); setError('');
    if (!market) return;
    api<ForecastReply>(`/items/${encodeURIComponent(itemKey)}/forecast?${new URLSearchParams({ market_id: market })}`)
      .then(d => { if (active) { setData(d); onData?.(d); } }).catch(e => { if (active) setError(e.message); });
    return () => { active = false; };
  }, [market, itemKey, refresh, retry, onData]);
  if (!market) return <p className="chart-note">Select a market after the first scan to calculate a forecast.</p>;
  if (error) return <div role="alert" className="error-state">Forecast unavailable: {error}<button onClick={() => setRetry(n => n + 1)}>Retry forecast</button></div>;
  if (!data || data.market_id !== market || data.item_key !== itemKey) return <p role="status">Loading forecast…</p>;
  return <section className="forecast-panel" aria-label="Price forecast"><div className="forecast-heading"><h2>24-hour price forecast</h2><span>{data.realm} · {data.house} · Updated {time(new Date(data.generated_at * 1000).toISOString())}</span></div>
    <ForecastEvidence data={data} now={now}/>
    <ExportButton market={market} keys={[itemKey]} disabled={data.demo || !usable(data, now)} label="Download this item for addon"/>
    <details><summary>Install the addon cache</summary><p className="chart-note">{installNote}</p></details>
  </section>;
}

export function Forecasts({ market }: { market?: Market }) {
  return market ? <MarketForecasts key={market.id} market={market}/> : <section className="forecasts-page"><h1>Price forecasts</h1><p>Select a market in Settings after the first scan is accepted.</p></section>;
}
function MarketForecasts({ market }: { market: Market }) {
  const [q, setQ] = useState(''), [offset, setOffset] = useState(0), [data, setData] = useState<ForecastPage | null>(null);
  const [selected, setSelected] = useState<string | null>(null), [error, setError] = useState(''), [retry, setRetry] = useState(0);
  const now = useMarketTime(), refresh = useMarketRefresh();
  useEffect(() => {
    let active = true;
    setData(null); setError('');
    const timer = setTimeout(() => {
      api<ForecastPage>(`/forecasts?${new URLSearchParams({ market_id: market.id, q, offset: String(offset) })}`)
        .then(d => { if (active) { setData(d); setSelected(old => d.items.some(r => r.forecast.item_key === old) ? old : d.items[0]?.forecast.item_key ?? null); } })
        .catch(e => { if (active) setError(e.message); });
    }, 150);
    return () => { active = false; clearTimeout(timer); };
  }, [market.id, q, offset, retry, refresh]);
  const current = data?.items.find(r => r.forecast.item_key === selected);
  const exportKeys = data?.items.filter(r => usable(r.forecast, now)).map(r => r.forecast.item_key) ?? [];
  return <section className="forecasts-page"><h1>Price forecasts</h1>
    <p>{market.realm} · {market.house} · Tomorrow’s median asking prices</p>
    <details><summary>How forecasts work</summary><p className="chart-note">Uses full scans from the last 60 days. Early estimates require a fresh complete scan and three observed days with at least two scans, four hours apart, on each day. Morning, midday, and evening scans are suitable. Provisional trends use recent next-day price changes. Their accuracy is measured as new observations arrive. Missing observations stay missing. With longer history, model selection uses the unchanged-price baseline when alternatives do not improve accuracy. Historical error ranges are not probability intervals. Asking prices do not prove a sale or future profit.</p></details>
    <div className="forecast-controls"><label>Market item<input type="search" value={q} maxLength={200} onChange={e => { setQ(e.target.value); setOffset(0); }} placeholder="Name or item ID"/></label></div>
    {error && <p role="alert" className="error-state">{error}<button onClick={() => setRetry(n => n + 1)}>Retry</button></p>}
    {!data && !error && <p role="status">Loading market forecasts…</p>}
    {data && <><div className="table-scroll"><table className="market-table"><thead><tr><th>Item</th><th>Current median</th><th>24-hour estimate</th><th>Change</th><th>Historical error range</th><th>Evidence</th></tr></thead><tbody>
      {data.items.map(({ name, forecast: f }) => <tr key={f.item_key}><td><button className="text-button" aria-pressed={selected === f.item_key} onClick={() => setSelected(f.item_key)}>{name}</button><small> · {f.item_key}</small></td><td><WowMoney value={forecastFresh(f, now) ? f.current_median : null}/></td>
        <td>{usable(f, now) ? <><WowMoney value={f.rolling.price}/><br/><small>{methodLabel(f.rolling)}</small></> : !forecastFresh(f, now) ? 'Estimate expired' : 'Insufficient data'}</td>
        <td>{usable(f, now) ? percent(f.rolling.change_percent) : '—'}</td>
        <td>{usable(f, now) ? f.rolling.low && f.rolling.high ? <><WowMoney value={f.rolling.low}/> – <WowMoney value={f.rolling.high}/></> : 'Not enough comparisons' : '—'}</td>
        <td><small>{f.rolling.observations} hours · {f.rolling.validation_pairs} comparisons</small></td></tr>)}
    </tbody></table></div>
      {!data.items.length && <p>{q ? 'No market items match this search.' : 'No full-scan item history is available for this market. Collect scans to build forecast history.'}</p>}
      <div className="forecast-heading"><span>{data.total_count.toLocaleString()} matching items · page {Math.floor(offset / 30) + 1}</span><div><button disabled={!offset} onClick={() => setOffset(Math.max(0, offset - 30))}>Previous</button><button disabled={data.next_offset === null} onClick={() => setOffset(data.next_offset ?? 0)}>Next</button></div>
        <ExportButton market={market.id} keys={exportKeys} disabled={market.demo} label="Download this page for addon"/></div>
      <details><summary>Install the addon cache</summary><p className="chart-note">The download includes available forecasts on this page. {installNote}</p></details>
    </>}
    {current && <section className="forecast-panel"><h2>{current.name} · forecast evidence</h2><ForecastEvidence data={current.forecast} now={now}/></section>}
  </section>;
}
