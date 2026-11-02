import type { Price } from './api';
import { moneyParts, moneyText } from './money';

/** WoW denominations: 100 copper = 1 silver; 100 silver = 1 gold. */
export function WowMoney({ value }: { value: Price | null | undefined }) {
  const parts = moneyParts(value);
  if (!parts) return <span className="money muted">—</span>;
  const coins = [['gold', parts.gold, 'g'], ['silver', parts.silver, 's'], ['copper', parts.copper, 'c']] as const;
  return <span className="money" role="img" aria-label={moneyText(value)} title={moneyText(value)}>
    {parts.negative && <span aria-hidden="true">−</span>}
    {coins.filter(([unit, amount]) => unit === 'copper' || amount > 0n).map(([unit, amount, suffix]) =>
      <span className="money-denomination" aria-hidden="true" key={unit}>
        <span>{amount.toLocaleString('en-GB')}<span className="sr-only">{suffix}</span></span>
        <span className={`coin coin-${unit}`} />
      </span>)}
  </span>;
}
