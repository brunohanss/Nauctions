import { describe, expect, it } from 'vitest';
import { goldCopper, planStacks, scenarioProfit, type Listing } from './Strategy.helpers';
const listing = (stack: number, cost: string): Listing => ({ stack_count: stack, buyout_total_copper: cost, unit_price: { numerator: cost, denominator: stack } });
describe('trade scenarios', () => {
  it('buys whole stacks and respects both capital and quantity limits', () => {
    const result = planStacks([listing(20, '2000'), listing(5, '250'), listing(2, '300')], 1000n, { numerator: '100', denominator: '1' }, 10);
    expect(result.units).toBe(5);
    expect(result.cost).toBe(250n);
    expect(result.stacks).toHaveLength(1);
  });
  it('keeps large copper values exact and applies fees and total deposit loss once', () => {
    const cost = 9007199254740993n;
    const plan = planStacks([listing(1, cost.toString())], cost, { numerator: cost.toString(), denominator: '1' }, 1);
    expect(plan.cost).toBe(cost);
    expect(scenarioProfit({ numerator: '200', denominator: '1' }, 5, 500n, '5', 20n)).toBe(430n);
    expect(scenarioProfit({ numerator: '50', denominator: '1' }, 5, 500n, '5', 20n)).toBe(-283n);
  });
  it('requires explicit valid cost assumptions', () => {
    expect(goldCopper('12.3456')).toBe(123456n);
    expect(goldCopper('')).toBeNull();
    expect(goldCopper('-1')).toBeNull();
    expect(goldCopper('1.00001')).toBeNull();
    for (const fee of ['', '-1', '101', 'NaN']) expect(scenarioProfit({ numerator: '10', denominator: '1' }, 1, 1n, fee, 0n)).toBeNull();
  });
});
