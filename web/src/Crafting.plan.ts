import type { Price } from './api';
import { materialCost, sumCosts } from './Crafting.helpers';

export type PlanRecipe = {
  name: string;
  output: number;
  reagents: Record<string, number>;
};
export type PlanSource = { recipe?: PlanRecipe; reason?: string; prices?: Record<string, Price | null> };
export type PurchasePlan = Awaited<ReturnType<typeof buildPurchasePlan>>;

/** Expand known, fixed-yield recipes. Aggregate demand before rounding each craft. */
export async function buildPurchasePlan(
  reagents: Record<string, number>, crafts: number,
  prices: Record<string, Price | null>,
  load: (id: string) => Promise<PlanSource>,
) {
  const nodes = new Map<string, PlanSource>();
  const visiting: string[] = [];
  const cycles = new Set<string>();
  async function discover(id: string) {
    const cycle = visiting.indexOf(id);
    if (cycle >= 0) { visiting.slice(cycle).forEach(key => cycles.add(key)); return; }
    if (nodes.has(id)) return;
    if (nodes.size >= 80 || visiting.length >= 12) {
      nodes.set(id, { reason: 'Recipe expansion limit reached. Buy this component directly.' });
      return;
    }
    const source = await load(id);
    nodes.set(id, source);
    Object.assign(prices, source.prices);
    if (source.recipe) {
      visiting.push(id);
      for (const child of Object.keys(source.recipe.reagents)) await discover(child);
      visiting.pop();
    }
  }
  for (const id of Object.keys(reagents)) await discover(id);
  for (const id of cycles) nodes.set(id, { reason: 'Recipe cycle found. Buy this component directly.' });
  const seen = new Set<string>(), order: string[] = [];
  function visit(id: string) {
    if (seen.has(id)) return;
    seen.add(id);
    for (const child of Object.keys(nodes.get(id)?.recipe?.reagents ?? {})) visit(child);
    order.push(id);
  }
  Object.keys(reagents).forEach(visit);
  const demand: Record<string, number> = {};
  function add(id: string, quantity: number) {
    const total = (demand[id] ?? 0) + quantity;
    if (!Number.isSafeInteger(total) || total < 0) throw new Error('The batch exceeds the supported material quantity.');
    demand[id] = total;
  }
  for (const [id, quantity] of Object.entries(reagents)) add(id, quantity * crafts);
  const steps: { id: string; name: string; crafts: number; required: number; surplus: number }[] = [];
  const purchases: { id: string; units: number; unitPrice: Price | null; cost: Price | null; reason?: string }[] = [];
  for (const id of order.reverse()) {
    const source = nodes.get(id)!;
    const required = demand[id];
    if (source.recipe) {
      const count = Math.ceil(required / source.recipe.output);
      steps.push({ id, name: source.recipe.name, crafts: count, required, surplus: count * source.recipe.output - required });
      for (const [child, quantity] of Object.entries(source.recipe.reagents)) add(child, quantity * count);
    } else {
      const unitPrice = prices[id] ?? null;
      purchases.push({ id, units: required, unitPrice, cost: materialCost(unitPrice, required), reason: source.reason });
    }
  }
  purchases.sort((a, b) => Number(a.id) - Number(b.id));
  return { purchases, steps: steps.reverse(), total: sumCosts(purchases.map(row => row.cost)) };
}
