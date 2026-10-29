import { useEffect, useState } from 'react';
import { api, time, type Market } from './api';
import { WowMoney } from './market-ui';
import { useMarketRefresh, useMarketTime } from './marketClock';
import { forecastFresh, percent, type BetaItem, type Estimate, type ForecastReply, type Reference } from './forecast-types';
import { downloadForecasts } from './forecast-export';

function EstimateCard({ estimate, expired = false }: { estimate: Estimate; expired?: boolean }) {
  const usable = !expired && estimate.status !== 'insufficient';
  return <article className="forecast-card"><h3>{estimate.horizon_hours === 24 ? '24 hours' : `${estimate.horizon_hours / 24} days`}</h3>
    <strong>{usable ? estimate.price ? <WowMoney value={estimate.price}/> : percent(estimate.change_percent) : expired ? 'Estimate expired' : 'Insufficient data'}</strong>
    {usable && estimate.price && <p>{percent(estimate.change_percent)} · {estimate.direction}</p>}
    {usable && estimate.low && estimate.high && <p>Range: <WowMoney value={estimate.low}/> – <WowMoney value={estimate.high}/></p>}
    <p className="chart-note">{estimate.reason}</p>{usable && <small>{estimate.status === 'baseline' ? 'Unchanged-price baseline' : estimate.status === 'scenario' ? 'Beta scenario · unvalidated' : 'Experimental forecast'}{estimate.validation_pairs > 0 ? ` · ${estimate.validation_pairs} later comparisons` : ''}</small>}
  </article>;
}
export function ForecastPanel({ itemKey, market, day = 0 }: { itemKey: string; market: string; day?: number }) {
  const [data, setData] = useState<ForecastReply | null>(null), [error, setError] = useState(''), [busy, setBusy] = useState(false), [retry, setRetry] = useState(0);
  const now = useMarketTime(), refresh = useMarketRefresh();
  useEffect(() => { let active = true; setData(null); setError('');
    if (!market) return;
    api<ForecastReply>(`/items/${encodeURIComponent(itemKey)}/forecast?${new URLSearchParams({ market_id: market, day: String(day) })}`).then(d => { if (active) setData(d); }).catch(e => { if (active) setError(e.message); });
    return () => { active = false; };
  }, [market, itemKey, day, refresh, retry]);
  if (!market) return <p className="chart-note">Select a live market after the first scan to calculate a 24-hour forecast. Beta scenarios below remain available.</p>;
  if (error) return <div role="alert" className="error-state">Forecast unavailable: {error}<button onClick={() => setRetry(n => n + 1)}>Retry forecast</button></div>;
  if (!data) return <p role="status">Loading forecasts…</p>;
  const expired = !forecastFresh(data, now);
  const aligned = data.launch.some(e => e.status === 'beta_based');
  return <section className="forecast-panel" aria-label="Price forecasts"><div className="forecast-heading"><h2>24-hour live forecast</h2><span>{data.realm} · {data.house} · Updated {time(new Date(data.generated_at * 1000).toISOString())}</span></div>
    <EstimateCard estimate={data.rolling} expired={expired}/>
    {data.rolling.mean_absolute_log_error !== null && <p className="chart-note">Later-period mean absolute log error: {data.rolling.mean_absolute_log_error.toFixed(3)}. Baseline: {data.rolling.baseline_error?.toFixed(3)}. The range uses historical errors with a minimum ±0.05 log-price margin; it is not a calibrated probability interval.</p>}
    <h2>{aligned ? 'Launch-aligned beta outlook' : 'Launch scenario from the beta reference'}</h2>
    {!data.reference_scope_matches && <p className="chart-note">The bundled reference covers Normal Horde only. It is not applied to this auction house.</p>}
    <p className="chart-note">Scenario day {day} relative to October 3, 2026. That date is not a verified reset. Values between observations use log-linear interpolation; no values extend beyond the observed window. The scenario range assumes ±0.25 in log price and has no probability attached.</p>
    {data.beta_item?.variants_combined && <p className="chart-note">This beta item combines random variants. Its scenario shows relative changes only; it does not price an exact live variant.</p>}
    <div className="forecast-grid">{(aligned ? data.launch : data.scenario).map(e => <EstimateCard key={e.horizon_hours} estimate={e} expired={expired}/>)}</div>
    <div className="forecast-heading"><button disabled={busy || data.demo || expired} onClick={async () => { setBusy(true); try { await downloadForecasts(market, [itemKey], day); } catch (e) { setError(String(e)); } finally { setBusy(false); } }}>{busy ? 'Preparing addon cache…' : 'Download this item for addon'}</button><small>Separate cache addon · closes no files and changes no SavedVariables · expires in at most 45 minutes</small></div>
    <p className="chart-note">Close WoW, extract the downloaded folder into Interface/AddOns, enable Nauctions Forecast Cache, then start WoW. A new export replaces the previous cache. Stale and wrong-market estimates are hidden.</p>
    <p className="catalog-credit"><a href={data.reference.attribution.url} target="_blank" rel="noreferrer">{data.reference.attribution.text}</a>. Asking prices do not prove a sale.</p>
  </section>;
}
type BetaReply = { reference: Reference; items: { item: BetaItem; scenario: Estimate[] }[]; total_count: number; next_offset: number | null };
const dateLabel = (offset: number) => new Date(Date.UTC(2026, 9, 3 + offset)).toISOString().slice(0, 10);
export function Forecasts({ market, admin = false }: { market?: Market; admin?: boolean }) {
  const [q, setQ] = useState(''), [day, setDay] = useState(0), [offset, setOffset] = useState(0), [data, setData] = useState<BetaReply | null>(null), [selected, setSelected] = useState<BetaItem | null>(null), [error, setError] = useState(''), [retry, setRetry] = useState(0), [exporting, setExporting] = useState(false);
  useEffect(() => { let active = true; setData(null); setError(''); const timer = setTimeout(() => {
    api<BetaReply>(`/forecasts/beta?${new URLSearchParams({ q, offset: String(offset), day: String(day) })}`).then(d => { if (active) { setData(d); setSelected(old => old && d.items.some(r => r.item.id === old.id) ? old : d.items[0]?.item ?? null); } }).catch(e => { if (active) setError(e.message); });
  }, 150); return () => { active = false; clearTimeout(timer); }; }, [q, offset, day, retry]);
  return <section className="forecasts-page"><h1>Price forecasts</h1><p>Explore launch scenarios from Normal Horde beta history, and estimate tomorrow’s asking prices from your live market.</p>
    <div className="forecast-controls"><label>Beta item<input type="search" value={q} onChange={e => { setQ(e.target.value); setOffset(0); }} placeholder="Name or item ID"/></label><label>Scenario reference day<select value={day} onChange={e => setDay(Number(e.target.value))}>{Array.from({ length: 18 }, (_, n) => <option value={n} key={n}>Day {n} · {dateLabel(n)}</option>)}</select></label></div>
    <p className="chart-note">3,458 items · 11,298 daily summaries · October 3, 4, 11 and 20, 2026 · Classic Beta PvE 2, Horde. The older PvE realm is excluded from this reference. Missing dates are not treated as observations. Beta reset time and region are unknown.</p>
    {error && <p role="alert" className="error-state">{error}<button onClick={() => setRetry(n => n + 1)}>Retry</button></p>}
    {!data && !error && <p role="status">Loading beta history…</p>}
    {data && <><div className="table-scroll"><table className="market-table"><thead><tr><th>Item</th><th>Observed days</th><th>1-day scenario</th><th>3-day scenario</th><th>7-day scenario</th></tr></thead><tbody>{data.items.map(({ item, scenario }) => <tr key={item.id}><td><button className="text-button" aria-pressed={selected?.id === item.id} onClick={() => setSelected(item)}>{item.name}</button>{item.variants_combined && <small> · All random variants</small>}</td><td>{item.points.length}</td>{scenario.map(e => <td key={e.horizon_hours}>{percent(e.change_percent)}</td>)}</tr>)}</tbody></table></div>
      {!data.items.length && <p>No beta items match this search.</p>}
      <div className="forecast-heading"><span>{data.total_count.toLocaleString()} matching items · page {Math.floor(offset / 30) + 1}</span><div><button disabled={!offset} onClick={() => setOffset(Math.max(0, offset - 30))}>Previous</button><button disabled={data.next_offset === null} onClick={() => setOffset(data.next_offset ?? 0)}>Next</button></div>
      <button disabled={!market || market.demo || exporting || !data.items.length} onClick={async () => { if (!market) return; setExporting(true); try { await downloadForecasts(market.id, data.items.map(r => `i${r.item.id}`), day); } catch (e) { setError(String(e)); } finally { setExporting(false); } }}>{exporting ? 'Preparing cache…' : 'Download this page for addon'}</button></div>
      <p className="catalog-credit"><a href={data.reference.attribution.url} target="_blank" rel="noreferrer">{data.reference.attribution.text}</a>. Beta references are bundled in Nauctions AHDB. This download contains live forecasts for up to 30 selected items; install the separate cache addon while WoW is closed.</p>
    </>}
    {selected && <section className="forecast-panel"><h2>{selected.name} · observed beta history</h2><div className="table-scroll"><table><thead><tr><th>Date (UTC)</th><th>Minimum per unit</th><th>Median per unit</th><th>Listed quantity</th></tr></thead><tbody>{selected.points.map(([d, min, median, quantity]) => <tr key={d}><td>{dateLabel(d)}</td><td><WowMoney value={min ? { numerator: String(min), denominator: 1 } : null}/></td><td><WowMoney value={median ? { numerator: String(median), denominator: 1 } : null}/></td><td>{quantity ?? 'Unknown'}</td></tr>)}</tbody></table></div>
      <ForecastPanel itemKey={`i${selected.id}`} market={market?.id ?? ''} day={day}/>
    </section>}
    {admin && market && !market.demo && <LaunchSettings market={market.id}/>}
  </section>;
}
function LaunchSettings({ market }: { market: string }) {
  const [value, setValue] = useState(''), [message, setMessage] = useState(''), [busy, setBusy] = useState(false);
  useEffect(() => { let active = true; api<{ live_started_at: string | null }>(`/admin/forecast-data?market_id=${encodeURIComponent(market)}`).then(d => { if (active) setValue(d.live_started_at ?? ''); }).catch(e => { if (active) setMessage(e.message); }); return () => { active = false; }; }, [market]);
  return <details className="settings-section"><summary>Launch settings · administrator</summary><p>The bundled source has no verified beta reset time. Setting a live launch time alone does not establish a launch-aligned forecast. Comparable beta runs with known reset times must also be imported.</p><form onSubmit={async e => { e.preventDefault(); setBusy(true); try { const at = new Date(value); if (!Number.isFinite(at.getTime()) || !/(?:Z|[+-]\d{2}:\d{2})$/i.test(value)) throw new Error('Enter an ISO date and time with a timezone.'); await api('/admin/forecast-data', 'POST', { market_id: market, live_started_at: at.toISOString() }); setMessage('Launch time saved.'); } catch (e) { setMessage(String(e)); } finally { setBusy(false); } }}><label>Live launch time (ISO, with timezone)<input value={value} onChange={e => setValue(e.target.value)} placeholder="YYYY-MM-DDTHH:MM:SS+01:00" required/></label><button disabled={busy}>Save launch time</button></form>{message && <p role="status">{message}</p>}</details>;
}
