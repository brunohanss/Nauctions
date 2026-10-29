import { describe, expect, it } from 'vitest';
import { addonZip } from './forecast-export';
import { forecastFresh, percent } from './forecast-types';

describe('forecast delivery boundaries', () => {
  it('hides expired and future-dated estimates', () => {
    expect(forecastFresh({ generated_at: 100, expires_at: 200 }, 150000)).toBe(true);
    expect(forecastFresh({ generated_at: 100, expires_at: 200 }, 200000)).toBe(false);
    expect(forecastFresh({ generated_at: 1000, expires_at: 2000 }, 150000)).toBe(false);
    expect(forecastFresh({ generated_at: 100, expires_at: 100 }, 100000)).toBe(false);
    expect(percent(null)).toBe('Insufficient data');
  });
  it('builds a readable stored ZIP with bounded addon paths', () => {
    const data = addonZip([{ name: 'NauctionsForecastCache/ForecastData.lua', content: 'NauctionsForecastCache = {}\n' }]);
    const view = new DataView(data.buffer);
    expect(view.getUint32(0, true)).toBe(0x04034b50);
    expect(view.getUint32(data.length - 22, true)).toBe(0x06054b50);
    const central = view.getUint32(data.length - 6, true);
    expect(view.getUint32(central, true)).toBe(0x02014b50);
    const size = view.getUint32(18, true), nameLength = view.getUint16(26, true);
    expect(new TextDecoder().decode(data.slice(30 + nameLength, 30 + nameLength + size))).toBe('NauctionsForecastCache = {}\n');
    expect(() => addonZip([{ name: '../AuctionDB.lua', content: 'bad' }])).toThrow('Unexpected addon export file');
  });
});
