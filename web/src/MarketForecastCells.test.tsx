import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { MarketForecastCells } from './MarketForecastCells';
import { MarketClock } from './marketClock';
import type { ForecastReply } from './forecast-types';

const now = 1800000000;
const forecast = {
  generated_at: now, expires_at: now + 2700,
  current_median: { numerator: '100', denominator: '1' },
  rolling: { status: 'experimental', price: { numerator: '110', denominator: '1' }, change_percent: 10, reason: 'Test estimate' },
  rolling_12h: { status: 'experimental', price: { numerator: '120', denominator: '1' }, horizon_hours: 12, reason: '12-hour estimate' },
  rise_frequencies: [{ horizon_hours: 12, percent: 65, rises: 13, comparisons: 20, reason: 'Past observations' }],
} as ForecastReply;
function render(data: ForecastReply | undefined, asOf = now, loading = false, error = false) {
  return renderToStaticMarkup(<MarketClock asOf={new Date(asOf * 1000).toISOString()}><table><tbody><tr><MarketForecastCells forecast={data} loading={loading} error={error}/></tr></tbody></table></MarketClock>);
}
describe('market forecast columns', () => {
  it('shows predicted prices at both horizons and keeps the 24-hour graph and change', () => {
    const html = render(forecast);
    expect(html).toContain('+10.0%');
    expect(html).toContain('12h:');
    expect(html).toContain('aria-label="1s 20c"');
    expect(html).toContain('24h:');
    expect(html).toContain('aria-label="1s 10c"');
    expect(html).not.toContain('65%');
    expect(html).toContain('class="sparkline favorable"');
    expect(html).toContain('24-hour median forecast:');
    expect(html).toContain('Prices between the estimates are not separately predicted');
    // Three points share the historical sparkline renderer, including the independent 12h peak.
    expect(html).toContain('L36,2');
  });
  it('suppresses both metrics after expiry', () => {
    const html = render(forecast, now + 2701);
    expect(html).not.toContain('65%');
    expect(html).not.toContain('+10.0%');
    expect(html).not.toContain('class="sparkline');
    expect(html).not.toContain('aria-label="1s 20c"');
  });
  it('keeps request failure separate from loading and missing evidence', () => {
    expect(render(undefined, now, true)).toContain('Loading');
    expect(render(undefined, now, false, true)).toContain('Unavailable');
    expect(render(undefined)).toContain('Insufficient data');
  });
  it('shows the median baseline for a large percentage change on a low-priced item', () => {
    const html = render({ ...forecast, current_median: { numerator: '15000', denominator: '1' }, rolling: { ...forecast.rolling, price: { numerator: '34500', denominator: '1' }, change_percent: 130 } });
    expect(html).toContain('From median');
    expect(html).toContain('aria-label="1g 50s 0c"');
    expect(html).toContain('aria-label="3g 45s 0c"');
    expect(html).toContain('+130.0%');
  });
});
