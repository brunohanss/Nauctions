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
