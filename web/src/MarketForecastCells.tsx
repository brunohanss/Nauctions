import { TableCell } from './components/controls';
import { useEffect, useState } from 'react';
import { api } from './api';
import { useMarketTime } from './marketClock';
import { forecastFresh, percent, type ForecastReply } from './forecast-types';

export function useMarketForecasts(market: string, keys: string[], refresh: number) {
  const signature = JSON.stringify(keys);
  const [state, setState] = useState<{ signature: string; market: string; rows: ForecastReply[]; error: boolean }>({ signature: '', market: '', rows: [], error: false });
  useEffect(() => {
    let active = true;
    setState({ signature: '', market, rows: [], error: false });
    if (!market || signature === '[]') return;
    api<{ items: ForecastReply[] }>('/forecasts/batch', 'POST', { market_id: market, item_keys: JSON.parse(signature) })
      .then(data => { if (active) setState({ signature, market, rows: data.items, error: false }); })
      .catch(() => { if (active) setState({ signature, market, rows: [], error: true }); });
    return () => { active = false; };
  }, [market, signature, refresh]);
  return { rows: state.market === market && state.signature === signature ? state.rows : [], loading: !!market && (state.market !== market || state.signature !== signature), error: state.error };
}

export function MarketForecastCells({ forecast, loading, error }: { forecast?: ForecastReply; loading: boolean; error: boolean }) {
  const now = useMarketTime();
  const fresh = !!forecast && forecastFresh(forecast, now);
  const usable = fresh && ['baseline', 'experimental'].includes(forecast.rolling.status) && forecast.rolling.price !== null;
  const missing = loading ? 'Loading…' : error ? 'Unavailable' : 'Insufficient data';
  return <><TableCell className="numeric market-metric" title={forecast?.rolling.reason}>
    {usable ? percent(forecast.rolling.change_percent) : <span className="metric-unavailable">{missing}</span>}
  </TableCell><TableCell className="numeric market-metric">{[12, 24].map(horizon => {
    const frequency = fresh ? forecast.rise_frequencies?.find(row => row.horizon_hours === horizon) : undefined;
    return <small key={horizon} title={frequency ? `${frequency.rises} rises in ${frequency.comparisons} comparable historical periods. ${frequency.reason} Not a forecast probability.` : 'Requires enough comparable historical periods and a fresh complete scan.'}>
      {horizon}h: {frequency?.percent != null && Number.isFinite(frequency.percent) ? `${frequency.percent.toFixed(0)}% (${frequency.comparisons})` : '—'}
    </small>;
  })}</TableCell></>;
}
