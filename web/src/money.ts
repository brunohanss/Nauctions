import type { Price } from './api';

/** Keep rational calculations exact. Round halves away from zero for display only. */
export function moneyParts(value: Price | null | undefined) {
  if (!value) return null;
  try {
    for (const part of [value.numerator, value.denominator]) {
      if (typeof part === 'number' && !Number.isSafeInteger(part)) return null;
    }
    const n = BigInt(value.numerator), d = BigInt(value.denominator);
    if (d <= 0n) return null;
    const amount = ((n < 0n ? -n : n) + d / 2n) / d;
    return { negative: n < 0n && amount > 0n, gold: amount / 10000n, silver: amount / 100n % 100n, copper: amount % 100n };
  } catch { return null; }
}

export function moneyText(value: Price | null | undefined): string {
  const p = moneyParts(value);
  if (!p) return '—';
  return `${p.negative ? '−' : ''}${p.gold ? `${p.gold.toLocaleString('en-GB')}g ` : ''}${p.silver ? `${p.silver}s ` : ''}${p.copper}c`;
}
