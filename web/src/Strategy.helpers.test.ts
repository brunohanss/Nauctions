import { describe, expect, it } from 'vitest';
import { goldCopper, planStacks, scenarioProfit, strategyUnitProfit, compareStrategyPrices, type Listing } from './Strategy.helpers';
const listing = (stack: number, cost: string): Listing => ({ stack_count: stack, buyout_total_copper: cost, unit_price: { numerator: cost, denominator: stack } });
describe('trade scenarios', () => {
  it('calculates per-unit profit after the auction cut without rounding fractions', () => {
    const result = strategyUnitProfit({ numerator: '101', denominator: '2' }, { numerator: '100', denominator: '1' });
    expect(result).toEqual({ numerator: '8900', denominator: '200' });
    expect(strategyUnitProfit(null, { numerator: '100', denominator: '1' })).toBeNull();
    expect(strategyUnitProfit({ numerator: '1', denominator: '0' }, { numerator: '100', denominator: '1' })).toBeNull();
    expect(strategyUnitProfit({ numerator: '100', denominator: '1' }, { numerator: '50', denominator: '1' })).toEqual({ numerator: '-5250', denominator: '100' });
  });
  it('sorts exact prices and negative profits and keeps missing estimates last', () => {
    const a = { numerator: '9007199254740993', denominator: '1' }, b = { numerator: '9007199254740994', denominator: '1' };
    expect(compareStrategyPrices(a, b, 'desc')).toBe(1);
    expect(compareStrategyPrices(a, b, 'asc')).toBe(-1);
    for (const direction of ['asc', 'desc'] as const) expect(compareStrategyPrices(null, a, direction)).toBe(1);
    expect(compareStrategyPrices({ numerator: '-5', denominator: '1' }, { numerator: '1', denominator: '2' }, 'desc')).toBe(1);
  });
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
