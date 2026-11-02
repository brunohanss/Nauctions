// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Strategy } from './Strategy';
import { api } from './api';
import { TooltipProvider } from './components/ui/tooltip';
import { MarketClock } from './marketClock';

vi.mock('./api', async importOriginal => ({ ...await importOriginal<typeof import('./api')>(), api: vi.fn() }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
const price = (value: number | null) => value === null ? null : { numerator: String(value), denominator: '1' };
const now = '2026-11-02T12:00:00Z';
const row = (name: string, entry: number | null, target: number | null, hours: number | null, strategy: string, action: string) => ({
  item: { item_key: name, name, observed_at: now },
  patterns: [{ detected: true, strategy, action, status: 'watch', entry_reference: price(entry), target: price(target), holding_hours: hours }],
});

it('defaults to net profit descending and sorts every column before displaying rows', async () => {
  vi.mocked(api).mockResolvedValue({ as_of: now, items: [
    row('Amber', 200, 300, 6, 'intraday_recovery', 'Review entry'),
    row('Unknown', null, null, null, 'market_dump', 'Watch'),
    row('Zinc', 100, 300, 48, 'weekly_cycle', 'Wait'),
  ] });
  render(<TooltipProvider><MarketClock asOf={now}><Strategy market="normal" query="" onItem={vi.fn()}/></MarketClock></TooltipProvider>);
  await screen.findByRole('link', { name: 'Amber' });
  const order = () => screen.getAllByRole('row').slice(1).map(row => within(row).getByRole('link').textContent);
  expect(order()).toEqual(['Zinc', 'Amber', 'Unknown']);
  expect(screen.getByRole('columnheader', { name: 'Expected profit / unit' }).getAttribute('aria-sort')).toBe('descending');
  const user = userEvent.setup();
  for (const [column, expected] of [
    ['Expected profit / unit', ['Amber', 'Zinc', 'Unknown']],
    ['Item', ['Amber', 'Unknown', 'Zinc']],
    ['Action', ['Amber', 'Zinc', 'Unknown']],
    ['Buy below', ['Amber', 'Zinc', 'Unknown']],
    ['Buy below', ['Zinc', 'Amber', 'Unknown']],
    ['Sell target', ['Amber', 'Zinc', 'Unknown']],
    ['Holding period', ['Amber', 'Zinc', 'Unknown']],
    ['Holding period', ['Zinc', 'Amber', 'Unknown']],
    ['Plan', ['Amber', 'Unknown', 'Zinc']],
  ] as const) {
    await user.click(screen.getByRole('button', { name: column }));
    expect(order()).toEqual(expected);
  }
});
