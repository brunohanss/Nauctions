import { Percentage } from './Percentage';
import { toast } from 'sonner';
import { DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem } from './components/ui/dropdown-menu';
import { Alert } from './components/ui/alert';
import { Button, Details } from './components/controls';
import { useEffect, useState } from 'react';
import { api, time } from './api';
import { WowMoney } from './market-ui';
import { useMarketRefresh, useMarketTime } from './marketClock';
import { forecastFresh, type ForecastReply } from './forecast-types';
import { downloadForecasts } from './forecast-export';

const methodLabel = (estimate: ForecastReply['rolling']) => estimate.range_kind.startsWith('limited_history') ? estimate.method === 'damped_daily_change' ? 'Provisional trend · limited history' : 'Basic estimate · limited history' : estimate.status === 'baseline' ? 'Unchanged-price baseline' : estimate.status === 'experimental' ? 'Experimental forecast' : 'Insufficient data';
const usable = (data: ForecastReply, now: number) => forecastFresh(data, now) && ['baseline', 'experimental'].includes(data.rolling.status) && data.rolling.price !== null;

export function ForecastEvidence({ data, now }: { data: ForecastReply; now: number }) {
  const estimate = data.rolling;
  const expired = !forecastFresh(data, now);
  return <><div className="forecast-grid">
    <article className="forecast-card"><h3>Current median</h3><strong><WowMoney value={expired ? null : data.current_median}/></strong></article>
    <article className="forecast-card"><h3>In 24 hours</h3><strong>{expired ? 'Estimate expired' : usable(data, now) ? <WowMoney value={estimate.price}/> : 'Insufficient data'}</strong>{usable(data, now) && <p><Percentage value={estimate.change_percent} baseline={0} signed missing="Insufficient data" /> · {estimate.direction}</p>}<small>{methodLabel(estimate)}</small></article>
    <article className="forecast-card"><h3>Historical error range</h3><strong>{usable(data, now) ? estimate.low && estimate.high ? <><WowMoney value={estimate.low}/> – <WowMoney value={estimate.high}/></> : 'Not enough comparisons' : '—'}</strong></article>
  </div>
    {data.rise_frequencies?.map(frequency => <p className="chart-note" key={frequency.horizon_hours}><strong>Historical rises ({frequency.horizon_hours}h): </strong>{expired ? 'Estimate expired' : frequency.percent === null ? 'Insufficient comparisons' : <><Percentage value={frequency.percent} /> · {frequency.rises}/{frequency.comparisons}</>}</p>)}
    <p className="chart-note">Historical frequency, not a probability.</p>
    <Details><summary>How this is calculated</summary>
      <p>{estimate.reason}</p>
      <p className="chart-note">{estimate.observations} observed hours · {estimate.validation_pairs} historical comparisons</p>
      <p className="chart-note">Prices use fresh complete scans, in copper per unit. The historical error range is not a probability interval. Asking prices do not prove a sale.</p>
      {data.rise_frequencies?.map(frequency => <p className="chart-note" key={frequency.horizon_hours}>{frequency.horizon_hours}h: {frequency.reason}</p>)}
      {estimate.mean_absolute_log_error !== null && <p className="chart-note">{estimate.range_kind.startsWith('limited_history') ? 'Historical' : 'Later-period'} mean absolute log error: {estimate.mean_absolute_log_error.toFixed(3)}. Unchanged-price baseline: {estimate.baseline_error?.toFixed(3)}.</p>}
    </Details>
  </>;
}

export function ForecastExportButton({ market, keys, disabled = false, label, menu = false }: { market: string; keys: string[]; disabled?: boolean; label: string; menu?: boolean }) {
 const [busy,setBusy]=useState(false),[error,setError]=useState('');
 const unavailable=disabled || busy || !keys.length;
 async function download(){setBusy(true);setError('');try {await downloadForecasts(market,keys);toast.success('Addon cache downloaded');} catch(e){setError(String(e));} finally{setBusy(false)}}
 return <>{menu ? <DropdownMenu><DropdownMenuTrigger asChild><Button disabled={busy}>Actions</Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem disabled={unavailable} onSelect={()=>void download()}>{busy?'Preparing addon cache...':label}</DropdownMenuItem></DropdownMenuContent></DropdownMenu> : <Button disabled={unavailable} onClick={()=>void download()}>{busy?'Preparing addon cache...':label}</Button>}{error && <Alert role="alert" className="error-state">{error}</Alert>}</>;
}
export const forecastInstallNote = 'Close WoW. Extract the ZIP into Interface/AddOns. Enable Nauctions Forecast Cache, then start WoW. Each export replaces the previous cache and expires within 45 minutes.';

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
  if (error) return <Alert role="alert" className="error-state">Forecast unavailable: {error}<Button onClick={() => setRetry(n => n + 1)}>Retry forecast</Button></Alert>;
  if (!data || data.market_id !== market || data.item_key !== itemKey) return <p role="status">Loading forecast…</p>;
  return <section className="forecast-panel" aria-label="Price forecast"><div className="forecast-heading"><h2>24-hour price forecast</h2><span>{data.realm} · {data.house} · Updated {time(new Date(data.generated_at * 1000).toISOString())}</span></div>
    <ForecastEvidence data={data} now={now}/>
    <ForecastExportButton market={market} keys={[itemKey]} disabled={data.demo || !usable(data, now)} label="Download this item for addon"/>
    <Details><summary>Install the addon cache</summary><p className="chart-note">{forecastInstallNote}</p></Details>
  </section>;
}
