import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { MarketForecastCells } from './MarketForecastCells';
import { MarketClock } from './marketClock';
import type { ForecastReply } from './forecast-types';

const now = 1800000000;
const forecast = {
  generated_at: now, expires_at: now + 2700,
  rolling: { status: 'experimental', price: { numerator: '110', denominator: '1' }, change_percent: 10, reason: 'Test estimate' },
  rise_frequencies: [{ horizon_hours: 12, percent: 65, rises: 13, comparisons: 20, reason: 'Past observations' }],
} as ForecastReply;
function render(data: ForecastReply | undefined, asOf = now, loading = false, error = false) {
  return renderToStaticMarkup(<MarketClock asOf={new Date(asOf * 1000).toISOString()}><table><tbody><tr><MarketForecastCells forecast={data} loading={loading} error={error}/></tr></tbody></table></MarketClock>);
}
describe('market forecast columns', () => {
  it('distinguishes price change from historical frequency and supplies sample size', () => {
    const html = render(forecast);
    expect(html).toContain('+10.0%');
    expect(html).toContain('65% (20)');
    expect(html).toContain('Not a forecast probability');
  });
  it('suppresses both metrics after expiry', () => {
    const html = render(forecast, now + 2701);
    expect(html).not.toContain('65%');
    expect(html).not.toContain('+10.0%');
  });
  it('keeps request failure separate from loading and missing evidence', () => {
    expect(render(undefined, now, true)).toContain('Loading');
    expect(render(undefined, now, false, true)).toContain('Unavailable');
    expect(render(undefined)).toContain('Insufficient data');
  });
});
