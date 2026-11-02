import type { Price } from './api';

export { moneyText as craftingMoney } from './money';

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

export function undercutEstimate(quote: {
  sale_unit_price: Price | null;
  output_units: number | null;
  material_cost: Price | null;
  gross_sale_value: Price | null;
} | null | undefined): { unitPrice: Price | null; revenue: Price | null; profit: Price | null } {
  const unavailable = { unitPrice: null, revenue: null, profit: null };
  if (!quote?.sale_unit_price) return unavailable;
  const n = BigInt(quote.sale_unit_price.numerator), d = BigInt(quote.sale_unit_price.denominator);
  if (d <= 0n || n <= d) return unavailable;
  const unitPrice = { numerator: String(n - d), denominator: String(d) };
  if (quote.output_units == null || quote.gross_sale_value == null) return { ...unavailable, unitPrice };
  const revenue = { numerator: String((n - d) * BigInt(quote.output_units)), denominator: String(d) };
  if (!quote.material_cost) return { unitPrice, revenue, profit: null };
  const costN = BigInt(quote.material_cost.numerator), costD = BigInt(quote.material_cost.denominator);
  if (costD <= 0n) return { unitPrice, revenue, profit: null };
  const profit = { numerator: String(BigInt(revenue.numerator) * costD - costN * d), denominator: String(d * costD) };
  return { unitPrice, revenue, profit };
}

/** Profit as a percentage of material cost (return on cost). */
export function profitPercent(profit: Price | null | undefined, cost: Price | null | undefined): Price | null {
  if (!profit || !cost) return null;
  const pn = BigInt(profit.numerator), pd = BigInt(profit.denominator);
  const cn = BigInt(cost.numerator), cd = BigInt(cost.denominator);
  if (pd <= 0n || cd <= 0n || cn <= 0n) return null;
  return { numerator: String(pn * cd * 100n), denominator: String(pd * cn) };
}

export function materialCost(unitPrice: Price | null | undefined, units: number): Price | null {
  return unitPrice ? { numerator: String(BigInt(unitPrice.numerator) * BigInt(units)), denominator: unitPrice.denominator } : null;
}

export function sumCosts(costs: (Price | null)[]): Price | null {
  let n = 0n, d = 1n;
  for (const cost of costs) {
    if (!cost || BigInt(cost.denominator) <= 0n) return null;
    const cd = BigInt(cost.denominator);
    n = n * cd + BigInt(cost.numerator) * d;
    d *= cd;
    let a = n < 0n ? -n : n, b = d;
    while (b) [a, b] = [b, a % b];
    if (a) { n /= a; d /= a; }
  }
  return { numerator: String(n), denominator: String(d) };
}
