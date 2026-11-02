import { discount, type Item } from './api';
import { TableCell } from './components/controls';
import { WowMoney } from './WowMoney';
import { Percentage } from './Percentage';

export function MarketMedianCells({ item }: { item: Item }) {
  return <>
    <TableCell className="numeric column-reference"><div className="median-values">
      <span><small>7d</small><WowMoney value={item.baseline.price}/></span>
      <span><small>Scan</small><WowMoney value={item.median_buyout}/></span>
    </div></TableCell>
    <TableCell className="numeric column-discount"><div className="median-values">
      <span><small>7d</small><Percentage value={item.discount_percentage == null ? null : -item.discount_percentage} baseline={0}/></span>
      <span><small>Scan</small><Percentage value={discount(item, item.median_buyout)} baseline={0}/></span>
    </div></TableCell>
  </>;
}
