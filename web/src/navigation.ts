import type { Market } from './api';

export function supportedMarkets(markets: Market[]): Market[] {
  return markets.filter(market => market.demo || (market.realm === 'Normal' && market.house === 'Horde'));
}

export function normalizeRoute(route: string): string {
  if (['/', '/opportunities', '/items', '/forecasts'].includes(route)) return '/market';
  return route;
}
