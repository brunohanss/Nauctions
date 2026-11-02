// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ItemTooltip, itemDescription } from './ItemTooltip';
import { TooltipProvider } from './components/ui/tooltip';
import { api } from './api';
vi.mock('./api', () => ({ api: vi.fn() }));
vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });
afterEach(() => { cleanup(); vi.clearAllMocks(); });

it('renders effect text and coin units without source markup or scripts', () => {
  expect(itemDescription('<b>Potion</b><br><script>bad()</script><span>Use: Restores health.</span><div>Sell Price: <span class="moneysilver">2</span></div>', 'Potion'))
    .toEqual(['Use: Restores health.', 'Sell Price: 2s']);
});

it('shows the bundled WoW description on hover and preserves item clicks', async () => {
  const click = vi.fn(), user = userEvent.setup();
  render(<TooltipProvider><ItemTooltip name="Greater Healing Potion" itemKey="i1710"><button onClick={click}>Potion</button></ItemTooltip></TooltipProvider>);
  await user.hover(screen.getByRole('button'));
  const tip = await screen.findByRole('tooltip');
  expect(tip.textContent).toContain('Restores 455 to 585 health.');
  expect(tip.textContent).toContain('Requires Level 21');
  expect(tip.textContent).toContain('Sell Price: 1s 25c');
  expect(api).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button'));
  expect(click).toHaveBeenCalledOnce();
});

it('opens from keyboard focus and closes with Escape', async () => {
  const user = userEvent.setup();
  render(<TooltipProvider><ItemTooltip name="Linen Cloth" itemId={2589}><a href="/items/i2589">Linen Cloth</a></ItemTooltip></TooltipProvider>);
  await user.tab();
  expect(await screen.findByRole('tooltip')).toBeTruthy();
  await user.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('tooltip')).toBeNull());
  expect(document.activeElement).toBe(screen.getByRole('link'));
});

it('loads unknown catalogue items on demand and handles unavailable descriptions', async () => {
  vi.mocked(api).mockRejectedValue(new Error('offline'));
  const user = userEvent.setup();
  render(<TooltipProvider><ItemTooltip name="Unknown object" itemId={987654}><button>Unknown object</button></ItemTooltip></TooltipProvider>);
  expect(api).not.toHaveBeenCalled();
  await user.hover(screen.getByRole('button'));
  await waitFor(() => expect(screen.getByRole('tooltip').textContent).toContain('Description unavailable.'));
  expect(api).toHaveBeenCalledWith('/item-metadata/987654');
});
