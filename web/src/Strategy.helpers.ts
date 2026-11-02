import type { Price } from './api';

/** Per unit: sell at the target, pay the 5% auction cut, buy at the entry limit. */
export function strategyUnitProfit(entry: Price | null, target: Price | null): Price | null {
  if (!entry || !target) return null;
  try {
    const en = BigInt(entry.numerator), ed = BigInt(entry.denominator);
    const tn = BigInt(target.numerator), td = BigInt(target.denominator);
    if (en < 0n || tn < 0n || ed <= 0n || td <= 0n) return null;
    return { numerator: (tn * ed * 95n - en * td * 100n).toString(), denominator: (td * ed * 100n).toString() };
  } catch { return null; }
}

/** Keep missing prices last in both sort directions. Compare exact fractions. */
export function compareStrategyPrices(a: Price | null, b: Price | null, direction: 'asc' | 'desc'): number {
  if (!a || !b) return a ? -1 : b ? 1 : 0;
  const difference = BigInt(a.numerator) * BigInt(b.denominator) - BigInt(b.numerator) * BigInt(a.denominator);
  return (difference < 0n ? -1 : difference > 0n ? 1 : 0) * (direction === 'asc' ? 1 : -1);
}

export type Listing = { stack_count: number; buyout_total_copper: string; unit_price: Price };
export function goldCopper(input: string): bigint | null {
  if (!/^\d+(\.\d{1,4})?$/.test(input)) return null;
  const [whole, fraction = ''] = input.split('.');
  return BigInt(whole) * 10000n + BigInt(fraction.padEnd(4, '0'));
}
export function priceGold(p: Price | null): string {
  if (!p) return '';
  const copper = BigInt(p.numerator) / BigInt(p.denominator);
  return `${copper / 10000n}.${(copper % 10000n).toString().padStart(4, '0')}`;
}
export function planStacks(listings: Listing[], budget: bigint, limit: Price, maxUnits: number) {
  const n = BigInt(limit.numerator), d = BigInt(limit.denominator);
  const valid = listings.filter(l => Number.isSafeInteger(l.stack_count) && l.stack_count > 0 && /^\d+$/.test(l.buyout_total_copper) && BigInt(l.buyout_total_copper) > 0n);
  valid.sort((a, b) => {
    const left = BigInt(a.buyout_total_copper) * BigInt(b.stack_count), right = BigInt(b.buyout_total_copper) * BigInt(a.stack_count);
    return left < right ? -1 : left > right ? 1 : 0;
  });
  let cost = 0n, units = 0;
  const stacks: Listing[] = [];
  if (!Number.isSafeInteger(maxUnits) || maxUnits <= 0 || budget <= 0n || d <= 0n || n <= 0n) return { cost, units, stacks };
  for (const l of valid) {
    const total = BigInt(l.buyout_total_copper);
    if (total * d > n * BigInt(l.stack_count)) continue;
    if (cost + total > budget || units + l.stack_count > maxUnits) continue;
    cost += total; units += l.stack_count; stacks.push(l);
  }
  return { cost, units, stacks };
}
export function scenarioProfit(target: Price, units: number, cost: bigint, fee: string, deposits: bigint): bigint | null {
  if (!/^\d+(\.\d{1,2})?$/.test(fee) || !Number.isSafeInteger(units) || units <= 0) return null;
  const [whole, fraction = ''] = fee.split('.');
  const bps = BigInt(whole) * 100n + BigInt(fraction.padEnd(2, '0'));
  if (bps > 10000n || BigInt(target.denominator) <= 0n) return null;
  return BigInt(target.numerator) * BigInt(units) * (10000n - bps) / (BigInt(target.denominator) * 10000n) - cost - deposits;
}
