import { describe, expect, it } from 'vitest';
import { craftingMoney, quoteOptions } from './Crafting.helpers';
describe('crafting display and options', () => {
  it('keeps large exact amounts, negative profits and zero', () => {
    expect(craftingMoney({ numerator: '-280', denominator: '1' })).toBe('−2s 80c');
    expect(craftingMoney({ numerator: '0', denominator: '1' })).toBe('0c');
    expect(craftingMoney({ numerator: '10', denominator: '3' })).toBe('3.33c');
    expect(craftingMoney({ numerator: '900719925474099300', denominator: '1' })).toBe('90071992547409g 93s 0c');
    expect(craftingMoney(null)).toBe('—');
  });
  it('validates inputs before a quote request', () => {
    expect(quoteOptions('2', '5.25', '123', 'purchase')?.get('fee_bps')).toBe('525');
    for (const args of [['0', '5', '0', 'reference'], ['1', '101', '0', 'reference'], ['1', '5', '-1', 'reference'], ['1', '5', '18446744073709551616', 'purchase']]) {
      expect(quoteOptions(...args as [string, string, string, string])).toBeNull();
    }
  });
});
