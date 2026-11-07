import { api, ApiError, type Item } from './api';

export type MarketPage = { items: Item[]; next_cursor: string | null; total_count?: number };

/** Older servers do not accept the watchlist category. Preserve their sort order
 * and filter before applying the displayed page, rather than filtering one page. */
export async function marketPage(query: string, watched: readonly string[]): Promise<MarketPage> {
  try {
    return await api<MarketPage>(`/items?${query}`);
  } catch (error) {
    const params = new URLSearchParams(query);
    if (params.get('category') !== 'watchlist' || !(error instanceof ApiError)
      || error.status !== 400 || error.message !== 'invalid_category') throw error;

    const limit = Number(params.get('limit') || 20);
    const offset = Number(params.get('cursor') || 0);
    const keys = new Set(watched);
    if (!keys.size) return { items: [], next_cursor: null, total_count: 0 };
    params.set('category', 'all');
    params.set('limit', '100');
    params.delete('cursor');
    const matches: Item[] = [];
    // Follow every page. A database batch size is not a total market limit.
    const cursors = new Set<string>();
    while (true) {
      const cursor = params.get('cursor') || '0';
      if (cursors.has(cursor)) throw new Error('The server repeated a market page. Retry the search.');
      cursors.add(cursor);
      const result = await api<MarketPage>(`/items?${params}`);
      matches.push(...result.items.filter(item => keys.has(item.item_key)));
      if (!result.next_cursor) {
        return {
          items: matches.slice(offset, offset + limit),
          next_cursor: offset + limit < matches.length ? String(offset + limit) : null,
          total_count: matches.length,
        };
      }
      params.set('cursor', result.next_cursor);
    }
  }
}
