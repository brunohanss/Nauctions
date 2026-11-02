import type { Price } from './api';

/** Ratios compare with 100%; changes and margins compare with 0%. */
export function Percentage({ value, baseline = 100, digits = 1, signed = false, missing = '—', higherIsBetter = true }: {
  value: number | Price | null | undefined;
  baseline?: number;
  higherIsBetter?: boolean;
  digits?: number;
  signed?: boolean;
  missing?: string;
}) {
  const number = value == null ? NaN : typeof value === 'number' ? value : Number(value.denominator) > 0 ? Number(value.numerator) / Number(value.denominator) : NaN;
  if (!Number.isFinite(number)) return <span className="muted">{missing}</span>;
  const rounded = Number(number.toFixed(digits));
  const tone = rounded === baseline || rounded === 0 ? 'neutral' : (rounded > baseline) === higherIsBetter ? 'positive' : 'negative';
  return <span className={`percentage percentage-${tone}`}>{signed && rounded >= 0 ? '+' : ''}{rounded.toLocaleString('en-GB', { minimumFractionDigits: digits, maximumFractionDigits: digits })}%</span>;
}
