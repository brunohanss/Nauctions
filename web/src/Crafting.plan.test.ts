import { expect, it } from 'vitest';
import { buildPurchasePlan, type PlanSource } from './Crafting.plan';
import { craftingMoney } from './Crafting.helpers';

const price = (n: number) => ({ numerator: String(n), denominator: '1' });
const recipe = (name: string, output: number, reagents: Record<string, number>): PlanSource => ({ recipe: { name, output, reagents } });

it('expands bolts and merges shared thread before rounding intermediate craft yields', async () => {
  const catalog: Record<string, PlanSource> = {
    '10': recipe('Bolt', 2, { '20': 3 }),
    '11': recipe('Lining', 1, { '10': 1, '30': 2 }),
  };
  const plan = await buildPurchasePlan({ '10': 1, '11': 1, '30': 1 }, 3, { '20': price(10), '30': price(2) }, async id => catalog[id] ?? {});
  expect(plan.purchases.map(row => [row.id, row.units])).toEqual([['20', 9], ['30', 9]]);
  expect(plan.steps.find(step => step.id === '10')).toMatchObject({ crafts: 3, required: 6, surplus: 0 });
  expect(plan.steps.map(step => step.id)).toEqual(['10', '11']);
  expect(craftingMoney(plan.total)).toBe('1s 8c');
});

it('rounds bar yields up and includes surplus and the full material expense', async () => {
  const plan = await buildPurchasePlan({ '10': 3 }, 1, { '20': price(10) }, async id => id === '10' ? recipe('Smelt bars', 2, { '20': 4 }) : {});
  expect(plan.steps[0]).toMatchObject({ crafts: 2, required: 3, surplus: 1 });
  expect(plan.purchases[0].units).toBe(8);
  expect(craftingMoney(plan.total)).toBe('80c');
});

it('keeps ambiguous recipes as purchases and leaves missing prices unknown', async () => {
  const plan = await buildPurchasePlan({ '10': 2 }, 2, {}, async () => ({ reason: 'Multiple recipes' }));
  expect(plan.purchases[0]).toMatchObject({ id: '10', units: 4, reason: 'Multiple recipes', cost: null });
  expect(plan.steps).toEqual([]);
  expect(plan.total).toBeNull();
});

it('stops recipe cycles and buys the requested component without recursion', async () => {
  const catalog: Record<string, PlanSource> = { '10': recipe('A', 1, { '20': 1 }), '20': recipe('B', 1, { '10': 1 }) };
  const plan = await buildPurchasePlan({ '10': 2 }, 1, { '10': price(5) }, async id => catalog[id]);
  expect(plan.steps).toEqual([]);
  expect(plan.purchases).toHaveLength(1);
  expect(plan.purchases[0]).toMatchObject({ id: '10', units: 2 });
  expect(craftingMoney(plan.total)).toBe('10c');
});

it('lists bolts as linen bag components and buys the linen cloth for the whole batch', async () => {
  // Local demo recipe: each bag needs three bolts; each bolt needs two cloth.
  const plan = await buildPurchasePlan({ '2996': 3 }, 4, { '2589': price(15) }, async id =>
    id === '2996' ? recipe('Bolt of Linen Cloth', 1, { '2589': 2 }) : {});
  expect(plan.steps).toEqual([{ id: '2996', name: 'Bolt of Linen Cloth', crafts: 12, required: 12, surplus: 0 }]);
  expect(plan.purchases.map(row => [row.id, row.units])).toEqual([['2589', 24]]);
  expect(craftingMoney(plan.total)).toBe('3s 60c');
});

it('lists linen cloth directly when the selected recipe is the bolt itself', async () => {
  const plan = await buildPurchasePlan({ '2589': 2 }, 5, { '2589': price(15) }, async () => ({}));
  expect(plan.steps).toEqual([]);
  expect(plan.purchases.map(row => [row.id, row.units])).toEqual([['2589', 10]]);
  expect(craftingMoney(plan.total)).toBe('1s 50c');
});
