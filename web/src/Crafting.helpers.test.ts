import { describe, expect, it } from 'vitest';
import { craftingMoney, quoteOptions, undercutEstimate, profitPercent } from './Crafting.helpers';
describe('crafting display and options', () => {
  it('keeps large exact amounts, negative profits and zero', () => {
    expect(craftingMoney({ numerator: '-280', denominator: '1' })).toBe('−2s 80c');
    expect(craftingMoney({ numerator: '0', denominator: '1' })).toBe('0c');
    expect(craftingMoney({ numerator: '10', denominator: '3' })).toBe('3c');
    expect(craftingMoney({ numerator: '900719925474099300', denominator: '1' })).toBe('90,071,992,547,409g 93s 0c');
    expect(craftingMoney(null)).toBe('—');
  });
  it('validates inputs before a quote request', () => {
    expect(quoteOptions('2', '5.25', '123', 'purchase')?.get('fee_bps')).toBe('525');
    for (const args of [['0', '5', '0', 'reference'], ['1', '101', '0', 'reference'], ['1', '5', '-1', 'reference'], ['1', '5', '18446744073709551616', 'purchase']]) {
      expect(quoteOptions(...args as [string, string, string, string])).toBeNull();
    }
  });
});

it('undercuts each output by one copper and deducts batch material cost without a percentage cut', () => {
  const p = (n: string) => ({ numerator: n, denominator: '1' });
  const quote = { sale_unit_price: p('200'), output_units: 3, material_cost: p('360'), gross_sale_value: p('600') };
  const sale = undercutEstimate(quote);
  expect(craftingMoney(sale.unitPrice)).toBe('1s 99c');
  expect(craftingMoney(sale.revenue)).toBe('5s 97c');
  expect(craftingMoney(sale.profit)).toBe('2s 37c');
  expect(craftingMoney(undercutEstimate({ ...quote, material_cost: p('700') }).profit)).toBe('−1s 3c');
  expect(undercutEstimate({ ...quote, sale_unit_price: p('1') }).profit).toBeNull();
  expect(undercutEstimate({ ...quote, gross_sale_value: null }).profit).toBeNull();
  expect(undercutEstimate({ ...quote, material_cost: null }).profit).toBeNull();
  expect(undercutEstimate(null).profit).toBeNull();
});

it('computes signed return on cost and leaves zero or missing costs unavailable', () => {
  const p = (n: number) => ({ numerator: String(n), denominator: '1' });
  expect(profitPercent(p(50), p(200))).toEqual({ numerator: '5000', denominator: '200' });
  expect(profitPercent(p(-50), p(200))).toEqual({ numerator: '-5000', denominator: '200' });
  expect(profitPercent(p(50), p(0))).toBeNull();
  expect(profitPercent(null, p(200))).toBeNull();
});
