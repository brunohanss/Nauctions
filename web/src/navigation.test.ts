import { describe, expect, it } from 'vitest';
import { normalizeRoute } from './navigation';

describe('consolidated market navigation', () => {
  it.each(['/', '/opportunities', '/items', '/forecasts'])('opens Market for the old %s bookmark', route => {
    expect(normalizeRoute(route)).toBe('/market');
  });
  it.each(['/market', '/crafting', '/strategy', '/settings', '/items/i2589'])('preserves %s', route => {
    expect(normalizeRoute(route)).toBe(route);
  });
});
