export function normalizeRoute(route: string): string {
  if (['/', '/opportunities', '/items', '/forecasts'].includes(route)) return '/market';
  return route;
}
