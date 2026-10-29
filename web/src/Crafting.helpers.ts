import type { Price } from './api';

/** Round only for display. All calculation amounts arrive as decimal strings. */
export function craftingMoney(value: Price | null | undefined): string {
  if (!value) return '—';
  try {
    const numerator = BigInt(value.numerator), denominator = BigInt(value.denominator);
    if (denominator <= 0n) return '—';
    const negative = numerator < 0n;
    const hundredths = ((negative ? -numerator : numerator) * 100n + denominator / 2n) / denominator;
    const gold = hundredths / 1000000n, silver = hundredths / 10000n % 100n;
    const copper = hundredths % 10000n;
    return `${negative && hundredths ? '−' : ''}${gold ? `${gold}g ` : ''}${gold || silver ? `${silver}s ` : ''}${copper / 100n}${copper % 100n ? `.${(copper % 100n).toString().padStart(2, '0')}` : ''}c`;
  } catch { return '—'; }
}

export function quoteOptions(crafts: string, fee: string, extra: string, mode: string): URLSearchParams | null {
  if (!/^\d+$/.test(crafts) || !/^\d+(\.\d{1,2})?$/.test(fee) || !/^\d+$/.test(extra)) return null;
  const count = Number(crafts), basisPoints = Math.round(Number(fee) * 100);
  if (count < 1 || count > 10000 || basisPoints < 0 || basisPoints > 10000 || BigInt(extra) > 18446744073709551615n || !['reference', 'purchase'].includes(mode)) return null;
  return new URLSearchParams({ crafts, fee_bps: String(basisPoints), additional_cost_copper: extra, mode });
}

export function craftingPercent(value: Price | null | undefined): string {
  if (!value) return '—';
  const n = Number(value.numerator), d = Number(value.denominator);
  return d > 0 && Number.isFinite(n / d) ? `${(n / d).toLocaleString('en-GB', { maximumFractionDigits: 1 })}%` : '—';
}
