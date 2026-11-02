import { describe, expect, it } from 'vitest';
import { normalizeRoute, supportedMarkets } from './navigation';
import type { Market } from './api';

it('keeps the demo market available alongside the supported live market', () => {
  const markets = [
    { id: 'demo', realm: 'Nauctions Demo', house: 'Alliance', demo: true },
    { id: 'live', realm: 'Normal', house: 'Horde', demo: false },
    { id: 'other', realm: 'Other', house: 'Alliance', demo: false },
  ] as Market[];
  expect(supportedMarkets(markets).map(market => market.id)).toEqual(['demo', 'live']);
});

describe('consolidated market navigation', () => {
  it.each(['/', '/opportunities', '/items', '/forecasts'])('opens Market for the old %s bookmark', route => {
    expect(normalizeRoute(route)).toBe('/market');
  });
  it.each(['/market', '/crafting', '/strategy', '/settings', '/items/i2589'])('preserves %s', route => {
    expect(normalizeRoute(route)).toBe(route);
  });
});
