import { forecastFresh } from './forecast-types';
import { price, type Item, type Price } from "./api";

export function observedPoints(points: Item[]): Item[] {
  return points.filter(point => Number.isFinite(Date.parse(point.observed_at)))
    .sort((a, b) => Date.parse(a.observed_at) - Date.parse(b.observed_at));
}

export function canConnectObservations(previous: Item | undefined, current: Item, field: "min_buyout" | "median_buyout"): boolean {
  if (!previous || !previous[field] || !current[field]) return false;
  const gap = Date.parse(current.observed_at) - Date.parse(previous.observed_at);
  return gap > 0 && gap <= 45 * 60_000;
}

export function priceDifference(current: Price | null, reference: Price | null): string {
  if (!current || !reference) return "—";
  try {
    const n = BigInt(current.numerator) * BigInt(reference.denominator) - BigInt(reference.numerator) * BigInt(current.denominator);
    return `${n < 0n ? "−" : n > 0n ? "+" : ""}${price({ numerator: (n < 0n ? -n : n).toString(), denominator: (BigInt(current.denominator) * BigInt(reference.denominator)).toString() })}`;
  } catch { return "—"; }
}

export function numericPrice(value: Price | null | undefined): number | null {
  if (!value) return null;
  const n = Number(value.numerator), d = Number(value.denominator), result = n / d;
  return d > 0 && n >= 0 && Number.isFinite(result) ? result : null;
}

/** Forecast coordinates are endpoints only. No observations are invented between them. */
export function chartProjection(item: Item | undefined, forecast: import('./forecast-types').ForecastReply | null | undefined, now: number) {
  if (!item || !forecast || item.item_key !== forecast.item_key || item.freshness_status === 'stale' || !forecastFresh(forecast, now)) return null;
  const anchorAt = Date.parse(item.observed_at), targetAt = forecast.rolling.target_at * 1000;
  const anchor = numericPrice(item.median_buyout), current = numericPrice(forecast.current_median), target = numericPrice(forecast.rolling.price);
  if (!['baseline', 'experimental'].includes(forecast.rolling.status) || anchor === null || current !== anchor || target === null || !Number.isFinite(anchorAt) || anchorAt > now + 300_000 || now - anchorAt > 2700_000 || !Number.isFinite(targetAt) || targetAt <= Math.max(anchorAt, now)) return null;
  const low = numericPrice(forecast.rolling.low), high = numericPrice(forecast.rolling.high);
  return { anchorAt, targetAt, anchor, target, low: low !== null && high !== null && low <= target && target <= high ? low : null, high: low !== null && high !== null && low <= target && target <= high ? high : null };
}

export function belowReference(current: Price | null, reference: Price | null): string {
  const value = numericPrice(current), baseline = numericPrice(reference);
  if (value === null || baseline === null || baseline <= 0) return 'Reference unavailable';
  const difference = (1 - value / baseline) * 100;
  return `${Math.abs(difference).toFixed(1)}% ${difference >= 0 ? 'below' : 'above'} the 7d historical median`;
}
