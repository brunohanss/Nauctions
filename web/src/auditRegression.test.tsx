import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { logPayload } from './telemetryLogs';
import { depthFresh, MarketClock, observationAge } from './marketClock';
import { FreshnessIndicator, WowMoney } from './market-ui';
import { HistoryChart } from './Detail';
import type { Item } from './api';
afterEach(() => vi.restoreAllMocks());
describe('release audit regressions', () => {
  it('filters the complete outbound OTLP payload, including URLs with query and fragment secrets', () => {
    const url = 'https://example.test/?invite=DUMMY_INVITE#DUMMY_FRAGMENT';
    const payload = logPayload('nauctions.app_opened', 'info', { 'url.full': url, url, location: url, component: 'browser-ui', token: 'DUMMY_TOKEN' });
    const encoded = JSON.stringify(payload);
    expect(encoded).not.toContain('DUMMY');
    expect(encoded).not.toContain('url.full');
    expect(payload.resourceLogs[0].scopeLogs[0].logRecords[0].body.stringValue).toBe('nauctions.app_opened');
  });
  it('expires depth from observation time and keeps replay time fixed', () => {
    const asOf = Date.parse('2026-01-01T12:00:00Z');
    const depth = { available: true, observed_at: new Date(asOf - 2700_000).toISOString() };
    expect(depthFresh(depth, asOf)).toBe(true);
    expect(depthFresh(depth, asOf + 1000)).toBe(false);
    expect(observationAge(depth.observed_at, asOf)).toBe(2700);
    vi.spyOn(Date, 'now').mockReturnValue(Date.parse('2026-10-29T12:00:00Z'));
    const html = renderToStaticMarkup(<MarketClock asOf={new Date(asOf).toISOString()}><FreshnessIndicator seconds={1} observedAt={depth.observed_at}/></MarketClock>);
    expect(html).toContain('45m'); expect(html).not.toContain('Stale');
    const stale = renderToStaticMarkup(<MarketClock asOf={new Date(asOf + 1000).toISOString()}><FreshnessIndicator seconds={1} observedAt={depth.observed_at}/></MarketClock>);
    expect(stale).toContain('Stale');
  });
  it('renders replay observations inside the visible chart months after replay', () => {
    const end = Date.parse('2026-01-01T12:00:00Z');
    vi.spyOn(Date, 'now').mockReturnValue(Date.parse('2026-10-29T12:00:00Z'));
    const point = { observed_at: new Date(end - 3600_000).toISOString(), median_buyout: { numerator: '100', denominator: '1' }, min_buyout: { numerator: '50', denominator: '1' }, total_units: 1, quality: 'complete', source_kind: 'demo' } as Item;
    const html = renderToStaticMarkup(<HistoryChart points={[point]} range="24h" visible={{median:true,minimum:true,quantity:true}} asOf={end}/>);
    expect(html).toContain('<svg');
    // At 23/24 of a 836-wide plot with an 82px left margin.
    expect(html).toContain('883.166');
    expect(html).not.toContain('NaN');
  });
  it('shows valid zero money for breakeven scenarios', () => {
    expect(renderToStaticMarkup(<WowMoney value={{numerator:'0',denominator:'1'}}/>)).toContain('0c');
  });
});
