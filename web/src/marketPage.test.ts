import { afterEach, expect, it, vi } from 'vitest';
import { api, ApiError, type Item } from './api';
import { marketPage } from './marketPage';

vi.mock('./api', async original => ({ ...await original<typeof import('./api')>(), api: vi.fn() }));
afterEach(() => vi.resetAllMocks());
const item = (key: string) => ({ item_key: key } as Item);

it('filters every legacy page before calculating the watched page and count', async () => {
  vi.mocked(api).mockRejectedValueOnce(new ApiError(400, 'invalid_category'))
    .mockResolvedValueOnce({ items: [item('unwatched'), item('first')], next_cursor: '100' })
    .mockResolvedValueOnce({ items: [item('second'), item('third')], next_cursor: null });
  const result = await marketPage('market_id=realm&category=watchlist&sort=current&direction=desc&q=ore&limit=1&cursor=1', ['first', 'second', 'third']);
  expect(result).toEqual({ items: [item('second')], next_cursor: '2', total_count: 3 });
  const fallback = new URL(vi.mocked(api).mock.calls[1][0], 'http://local');
  expect(Object.fromEntries(fallback.searchParams)).toEqual({ market_id: 'realm', category: 'all', sort: 'current', direction: 'desc', q: 'ore', limit: '100' });
  expect(new URL(vi.mocked(api).mock.calls[2][0], 'http://local').searchParams.get('cursor')).toBe('100');
});

it('returns an empty watchlist without loading all market pages', async () => {
  vi.mocked(api).mockRejectedValueOnce(new ApiError(400, 'invalid_category'));
  expect(await marketPage('category=watchlist', [])).toEqual({ items: [], next_cursor: null, total_count: 0 });
  expect(api).toHaveBeenCalledTimes(1);
});

it('uses the current server result directly', async () => {
  const page = { items: [item('saved')], next_cursor: null, total_count: 1 };
  vi.mocked(api).mockResolvedValueOnce(page);
  expect(await marketPage('category=watchlist', ['saved'])).toBe(page);
  expect(api).toHaveBeenCalledTimes(1);
});

it('keeps network and authentication errors visible', async () => {
  const failure = new ApiError(401, 'login_required');
  vi.mocked(api).mockRejectedValueOnce(failure);
  await expect(marketPage('category=watchlist', ['saved'])).rejects.toBe(failure);
  expect(api).toHaveBeenCalledTimes(1);
});
