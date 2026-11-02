import { Percentage } from './Percentage';
import { TableCell } from './components/controls';
import { useEffect, useState } from 'react';
import { api } from './api';
import { PriceSparkline, WowMoney } from './market-ui';
import { useMarketTime } from './marketClock';
import { forecastFresh, type ForecastReply } from './forecast-types';

export function useMarketForecasts(market: string, keys: string[], refresh: number) {
  const signature = JSON.stringify(keys);
  const [state, setState] = useState<{ signature: string; market: string; rows: ForecastReply[]; error: boolean }>({ signature: '', market: '', rows: [], error: false });
  useEffect(() => {
    let active = true;
    if (!market || signature === '[]') {
      setState({ signature, market, rows: [], error: false });
      return;
    }
    setState(previous => previous.market === market && previous.signature === signature
      ? { ...previous, error: false }
      : { signature: '', market, rows: [], error: false });
    api<{ items: ForecastReply[] }>('/forecasts/batch', 'POST', { market_id: market, item_keys: JSON.parse(signature) })
      .then(data => { if (active) setState({ signature, market, rows: data.items, error: false }); })
      .catch(() => { if (active) setState(previous => ({ signature, market, rows: previous.market === market && previous.signature === signature ? previous.rows : [], error: true })); });
    return () => { active = false; };
  }, [market, signature, refresh]);
  return { rows: state.market === market && state.signature === signature ? state.rows : [], loading: !!market && (state.market !== market || state.signature !== signature), error: state.error };
}

export function MarketForecastCells({ forecast, loading, error, part = 'all' }: { forecast?: ForecastReply; loading: boolean; error: boolean; part?: 'all' | 'forecast' | 'prices' }) {
  const now = useMarketTime();
  const fresh = !!forecast && forecastFresh(forecast, now);
  const usable = fresh && ['baseline', 'experimental'].includes(forecast.rolling.status) && forecast.rolling.price !== null;
  const missing = loading ? 'Loading…' : error ? 'Unavailable' : 'Insufficient data';
  return <>{part !== 'prices' && <TableCell className="numeric market-metric" title={forecast?.rolling.reason}>
    {usable ? <div className="market-forecast"><ForecastSparkline forecast={forecast}/><small className="muted">From median <WowMoney value={forecast.current_median}/></small><Percentage value={forecast.rolling.change_percent} baseline={0} signed missing="Insufficient data" /></div> : <span className="metric-unavailable">{missing}</span>}
  </TableCell>}{part !== 'forecast' && <TableCell className="numeric market-metric">{[12, 24].map(horizon => {
    const estimate = horizon === 12 ? forecast?.rolling_12h : forecast?.rolling;
    const available = fresh && estimate && ['baseline', 'experimental'].includes(estimate.status) && estimate.price !== null;
    return <small key={horizon} title={estimate?.reason}>
      {horizon}h: {available ? <WowMoney value={estimate.price}/> : <span className="metric-unavailable">{missing}</span>}
    </small>;
  })}</TableCell>}</>;
}

function ForecastSparkline({ forecast }: { forecast: ForecastReply }) {
  if (!forecast.current_median || !forecast.rolling.price) return null;
  const at = forecast.generated_at * 1000;
  const short = forecast.rolling_12h;
  const points = [
    { observed_at: new Date(at).toISOString(), median_buyout: forecast.current_median },
    ...(short && ['baseline', 'experimental'].includes(short.status) && short.price ? [{ observed_at: new Date(at + 12 * 3600000).toISOString(), median_buyout: short.price }] : []),
    { observed_at: new Date(at + 24 * 3600000).toISOString(), median_buyout: forecast.rolling.price },
  ];
  return <PriceSparkline points={points} higherIsBetter label="24-hour median forecast: current price and 12h / 24h estimates" description="Lines connect the current median and available forecast estimates. Prices between the estimates are not separately predicted."/>;
}
