import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { ForecastEvidence, ForecastPanel } from './Forecasts';
import type { ForecastReply } from './forecast-types';

const fixture: ForecastReply = {
  schema_version: 1, market_id: 'market', realm: 'Realm', house: 'Horde', item_key: 'i1',
  generated_at: 100, expires_at: 200, demo: false, rise_frequencies: [], current_median: { numerator: '100', denominator: '1' },
  rolling: { status: 'baseline', reason: 'Other methods did not improve accuracy.', method: 'last_price',
    horizon_hours: 24, target_at: 86500, price: { numerator: '100', denominator: '1' },
    low: { numerator: '90', denominator: '1' }, high: { numerator: '110', denominator: '1' },
    change_percent: 0, direction: 'stable', observations: 480, validation_pairs: 20,
    mean_absolute_log_error: 0, baseline_error: 0, range_kind: 'historical_error' },
};
describe('market forecast evidence', () => {
  it('shows the validated baseline, error evidence, and observed coverage', () => {
    const html = renderToStaticMarkup(<ForecastEvidence data={fixture} now={150000}/>);
    expect(html).toContain('Unchanged-price baseline');
    expect(html).toContain('480 observed hours');
    expect(html).toContain('20 historical comparisons');
    expect(html).toContain('+0.0%');
    expect(html).toContain('not a probability interval');
    expect(html).toContain('Historical frequency, not a probability.');
    expect(html).toContain('How this is calculated');
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain('<details open');
  });
  it('hides forecast prices and changes when the cache expires', () => {
    const html = renderToStaticMarkup(<ForecastEvidence data={fixture} now={200000}/>);
    expect(html).toContain('Estimate expired');
    expect(html).not.toContain('+0.0%');
    expect(html).not.toContain('1s 10c');
  });
  it('hides historical rise percentages after scan freshness expires', () => {
    const data = { ...fixture, rise_frequencies: [{ horizon_hours: 12 as const, status: 'available', reason: 'Observed past changes.', percent: 75, rises: 3, comparisons: 4, method: 'historical' }] };
    const html = renderToStaticMarkup(<ForecastEvidence data={data} now={200000}/>);
    expect(html).not.toContain('75.0%');
    expect(html).toContain('Historical rises (12h)');
    expect(html).toContain('Estimate expired');
  });
  it('keeps the insufficient-history reason available', () => {
    const data = { ...fixture, rolling: { ...fixture.rolling, status: 'insufficient', price: null, reason: 'At least three days are required.' } };
    const html = renderToStaticMarkup(<ForecastEvidence data={data} now={150000}/>);
    expect(html).toContain('Insufficient data');
    expect(html).toContain('At least three days are required.');
    expect(html).not.toContain('+0.0%');
  });
  it('labels sparse-history estimates without claiming later-period validation', () => {
    const data = { ...fixture, rolling: { ...fixture.rolling, range_kind: 'limited_history_error_range_not_probability' } };
    const html = renderToStaticMarkup(<ForecastEvidence data={data} now={150000}/>);
    expect(html).toContain('Basic estimate · limited history');
    expect(html).toContain('Historical mean absolute log error');
    expect(html).not.toContain('Later-period');
  });
  it('does not invent an error range when daily comparisons are unavailable', () => {
    const data = { ...fixture, rolling: { ...fixture.rolling, low: null, high: null, range_kind: 'limited_history_no_comparisons', mean_absolute_log_error: null, baseline_error: null } };
    const html = renderToStaticMarkup(<ForecastEvidence data={data} now={150000}/>);
    expect(html).toContain('Basic estimate · limited history');
    expect(html).toContain('Not enough comparisons');
    expect(html).not.toContain('Insufficient data');
  });
  it('identifies an early trend as provisional before later accuracy checks exist', () => {
    const data = { ...fixture, rolling: { ...fixture.rolling, status: 'experimental', method: 'damped_daily_change', low: null, high: null, range_kind: 'limited_history_no_comparisons', validation_pairs: 0, mean_absolute_log_error: null, baseline_error: null } };
    const html = renderToStaticMarkup(<ForecastEvidence data={data} now={150000}/>);
    expect(html).toContain('Provisional trend · limited history');
    expect(html).toContain('Not enough comparisons');
    expect(html).not.toContain('Later-period');
  });
  it('asks for a selected market before showing forecasts', () => {
    expect(renderToStaticMarkup(<ForecastPanel market="" itemKey="i2589"/>)).toContain('Select a market after the first scan');
  });
});
