import { routeUrl } from './desktop';
import { Alert } from './components/ui/alert';
import { Empty } from './components/ui/empty';
import { Pager, Button, Input, Label, Select, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, Details } from './components/controls';
import { useMarketTime, useMarketRefresh, depthFresh, observationAge } from './marketClock';
import { useEffect, useState } from 'react';
import { api, time, type Item, type Price } from './api';
import { ItemIcon, WowMoney } from './market-ui';
import { StrategyChart, type HistoricalPair } from './StrategyChart';
import { goldCopper, priceGold, planStacks, scenarioProfit, type Listing } from './Strategy.helpers';

type Pattern = {
  detected: boolean; strategy: string; status: string; action: string; reason: string;
  entry_reference: Price | null; target: Price | null; conservative_target: Price | null;
  holding_hours: number | null; holding_range_hours: [number, number] | null; historical_pairs: HistoricalPair[]; pairs: number; training_pairs: number;
  validation_pairs: number; validation_positive: number; historical_gross_change: number | null;
  coverage_days: number; observed_weeks: number;
};
type Cell = { weekday: number; hour: number; price: Price; quantity: number; observations: number; weeks: number };
type Research = { item: Item; patterns: Pattern[]; heatmap: Cell[] };
type Response = { items: Research[]; as_of: string };
const names: Record<string, string> = { weekly_cycle: 'Weekly cycle', intraday_recovery: 'Intraday recovery', market_dump: 'Market dump' };
const states: Record<string, string> = { watch: 'Watch', preliminary: 'Preliminary', insufficient: 'Insufficient data' };
const holdingPeriod = (hours: number | null) => hours == null ? 'Not established' : hours <= 24 ? 'A few hours' : 'A few days';

export function Strategy({ market, query, onItem }: { market: string; query: string; onItem: (key: string) => void }) {
  const now = useMarketTime(), refresh = useMarketRefresh();
  const [data, setData] = useState<Response | null>(null), [error, setError] = useState(''), [loading, setLoading] = useState(true), [retry, setRetry] = useState(0);
  const [horizon, setHorizon] = useState('all'), [selected, setSelected] = useState('');
  const [page, setPage] = useState(0);
  useEffect(() => { setPage(0); setSelected(''); }, [market, query, horizon]);
  useEffect(() => {
    let active = true; setLoading(true); setError('');
    if (!market) { setLoading(false); return; }
    const timer = setTimeout(() => api<Response>(`/strategies?${new URLSearchParams({ market_id: market, q: query })}`)
      .then(d => { if (active) setData(d); }).catch(e => { if (active) setError(e.message); }).finally(() => { if (active) setLoading(false); }), 200);
    return () => { active = false; clearTimeout(timer); };
  }, [market, query, retry, refresh]);
  const detected = (data?.items ?? []).flatMap(r => r.patterns.map(p => ({ ...r, pattern: p, id: `${r.item.item_key}:${p.strategy}` })))
    .map(r => observationAge(r.item.observed_at, now) > 2700 && r.pattern.action === 'Review entry' ? { ...r, pattern: { ...r.pattern, action: 'Refresh scan' } } : r)
    .filter(r => r.pattern.detected)
    .sort((a, b) => b.pattern.validation_positive - a.pattern.validation_positive || a.item.name.localeCompare(b.item.name));
  const rows = detected.filter(r => horizon === 'all' || (r.pattern.holding_hours != null && (horizon === 'hours' ? r.pattern.holding_hours <= 24 : r.pattern.holding_hours > 24)));
  const selection = rows.find(r => r.id === selected);
  const visibleRows = rows.slice(page * 20, page * 20 + 20);
  if (selection) return <section className="strategy-page"><Button className="text-button strategy-back" onClick={() => setSelected('')}>Back to strategies</Button><TradePlan key={`${market}:${selection.id}`} research={selection} pattern={selection.pattern} market={market} asOf={data!.as_of} onClose={() => setSelected('')}/></section>;
  return <section className="strategy-page">
    <div className="strategy-heading"><h1>Strategy</h1></div>
    {loading ? <Empty className="empty-state" role="status">Looking for strategies…</Empty> : error ? <Alert role="alert" className="error-state">{error}<Button onClick={() => setRetry(n => n + 1)}>Retry</Button></Alert> : !detected.length ? <Empty className="empty-state" role="status">No profitable strategy yet</Empty> : <>
      <div className="strategy-filters"><Label>Holding period<Select value={horizon} onChange={e => setHorizon(e.target.value)}><option value="all">Any period</option><option value="hours">A few hours</option><option value="days">A few days</option></Select></Label></div>
      {rows.length > 0 && <div className="table-scroll strategy-table"><Table className="market-table"><TableHeader><TableRow><TableHead>Item</TableHead><TableHead>Action</TableHead><TableHead className="numeric">Buy below</TableHead><TableHead className="numeric">Sell target</TableHead><TableHead>Holding period</TableHead><TableHead>Plan</TableHead></TableRow></TableHeader><TableBody>{visibleRows.map(r => <TableRow key={r.id} className={r.id === selected ? 'strategy-selected' : ''}>
        <TableCell><div className="item-cell"><ItemIcon name={r.item.name} icon={r.item.icon}/><span><a href={routeUrl(`/items/${encodeURIComponent(r.item.item_key)}`)} onClick={e => { if(e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey)return; e.preventDefault(); onItem(r.item.item_key); }}>{r.item.name}</a><small>{names[r.pattern.strategy]}</small></span></div></TableCell>
        <TableCell><strong>{r.pattern.action}</strong><small className={`strategy-status ${r.pattern.status}`}>{states[r.pattern.status]}</small></TableCell>
        <TableCell className="numeric"><WowMoney value={r.pattern.entry_reference}/></TableCell><TableCell className="numeric"><WowMoney value={r.pattern.target}/></TableCell>
        <TableCell title={r.pattern.holding_hours ? `About ${r.pattern.holding_hours} hours` : undefined}>{holdingPeriod(r.pattern.holding_hours)}</TableCell>
        <TableCell><Button aria-expanded={r.id === selected} onClick={() => setSelected(r.id === selected ? '' : r.id)}>Review<span className="sr-only"> {r.item.name}, {names[r.pattern.strategy]}</span></Button></TableCell>
      </TableRow>)}</TableBody></Table></div>}
      {!rows.length && <Empty className="empty-state" role="status">No profitable strategy yet</Empty>}
      {rows.length > 20 && <div className="table-footer"><span>{page * 20 + 1}–{Math.min(page * 20 + 20, rows.length)} of {rows.length} strategies</span><Pager><Button disabled={page === 0} onClick={() => { setPage(n => n - 1); setSelected(''); }}>Previous</Button><span aria-current="page">{page + 1}</span><Button disabled={(page + 1) * 20 >= rows.length} onClick={() => { setPage(n => n + 1); setSelected(''); }}>Next</Button></Pager></div>}
      
    </>}
  </section>;
}

function TradePlan({ research, pattern: p, market, asOf, onClose }: { research: Research; pattern: Pattern; market: string; asOf: string; onClose: () => void }) {
  const now = useMarketTime(), refresh = useMarketRefresh();
  const [depth, setDepth] = useState<{ available: boolean; listings: Listing[]; observed_at: string } | null>(null), [error, setError] = useState(''), [retry, setRetry] = useState(0);
  const [budget, setBudget] = useState(''), [maxUnits, setMaxUnits] = useState(''), [limit, setLimit] = useState(priceGold(p.entry_reference)), [fee, setFee] = useState(''), [deposits, setDeposits] = useState('');
  useEffect(() => { let active = true; setDepth(null); setError(''); api<typeof depth>(`/items/${encodeURIComponent(research.item.item_key)}/depth?${new URLSearchParams({ market_id: market })}`).then(d => { if (active) setDepth(d); }).catch(e => { if (active) setError(e.message); }); return () => { active = false; }; }, [market, research.item.item_key, retry, refresh]);
  useEffect(() => { document.getElementById('trade-plan')?.scrollIntoView({ behavior: 'smooth', block: 'start' }); document.getElementById('trade-title')?.focus({ preventScroll: true }); }, []);
  const budgetValue = goldCopper(budget), limitValue = goldCopper(limit), depositValue = goldCopper(deposits);
  const max = /^\d+$/.test(maxUnits) ? Number(maxUnits) : 0;
  const plan = depth && depthFresh(depth, now) && budgetValue != null && limitValue != null ? planStacks(depth.listings, budgetValue, { numerator: limitValue.toString(), denominator: '1' }, max) : null;
  const profit = plan && p.target && depositValue != null ? scenarioProfit(p.target, plan.units, plan.cost, fee, depositValue) : null;
  const conservative = plan && p.conservative_target && depositValue != null ? scenarioProfit(p.conservative_target, plan.units, plan.cost, fee, depositValue) : null;
  const money = (v: bigint | null) => v == null ? 'Enter costs' : <><span>{v < 0n ? '−' : ''}</span><WowMoney value={{ numerator: (v < 0n ? -v : v).toString(), denominator: '1' }}/></>;
  const range = p.holding_range_hours;
  const duration = range ? range[0] === range[1] ? `About ${range[0]} hours` : `${range[0]}-${range[1]} hours` : p.holding_hours ? `About ${p.holding_hours} hours` : 'Not established';
  const dateAt = (hours: number) => time(new Date(Date.parse(asOf) + hours * 3600000).toISOString());
  const ready = p.status === 'watch' && p.action === 'Review entry' && observationAge(research.item.observed_at, now) <= 2700;
  const dump = p.strategy === 'market_dump';
  return <article id="trade-plan" className="trade-plan strategy-simple" aria-labelledby="trade-title">
    <div className="strategy-heading"><div className="strategy-identity"><ItemIcon name={research.item.name} icon={research.item.icon}/><div><h1 id="trade-title" tabIndex={-1}>{research.item.name}</h1><p>{names[p.strategy]} / <span className={`strategy-status ${p.status}`}>{states[p.status]}</span></p></div></div><Button onClick={onClose}>Close</Button></div>
    <p className={`strategy-decision ${ready ? 'ready' : ''}`}>{ready ? 'Price is in the entry zone. Check the current listings before buying.' : p.action === 'Refresh scan' || observationAge(research.item.observed_at, now) > 2700 ? 'Refresh the market scan before considering an entry.' : p.status !== 'watch' ? 'Wait: this pattern has not passed the later price checks.' : 'Wait for the price to reach the entry zone.'}</p>
    <div className="strategy-steps">
      <section className="strategy-step enter-step"><span className="step-kicker">1 / ENTER</span><strong>{p.entry_reference ? <><WowMoney value={p.entry_reference}/><small>per unit or lower</small></> : 'Wait for recovery evidence'}</strong><p>{p.entry_reference ? <>Current lowest: <WowMoney value={research.item.min_buyout}/></> : 'A price dump alone is not an entry signal.'}</p><small>{ready ? `Entry check: ${time(asOf)}` : 'Only enter after a fresh scan and a confirmed pattern.'}</small></section>
      <section className="strategy-step hold-step"><span className="step-kicker">2 / HOLD</span><strong>{duration}</strong><p>{p.holding_hours && p.holding_hours > 24 ? 'A few days' : p.holding_hours ? 'A few hours' : 'No reliable timeframe yet'}</p><small>Based on observed historical intervals.</small></section>
      <section className="strategy-step exit-step"><span className="step-kicker">3 / EXIT</span><strong>{p.target ? <><WowMoney value={p.target}/><small>historical price target per unit</small></> : 'No exit target yet'}</strong><p>{range && p.target ? `${dateAt(range[0])} to ${dateAt(range[1])}` : 'Wait for a repeatable recovery.'}</p><small>{p.target ? 'Approximate window if you enter at the replay/current time.' : 'No recovery is assumed.'}</small></section>
    </div>
    <p className="strategy-plan-caption">{ready ? 'Plan to review' : 'Conditional plan'} / {time(asOf)} (Paris). Historical asking prices are references; fees and resale affect profit.</p>
    <StrategyChart itemKey={research.item.item_key} market={market} asOf={asOf} pairs={p.historical_pairs ?? []} entry={p.entry_reference} exit={p.target}/>
    {dump && <p className="strategy-note">Price and supply changed sharply. Watch the history for recovery; no ENTER or EXIT price has been established.</p>}
    <Details className="strategy-more"><summary>Why this pattern?</summary><p>{p.reason}</p><p>{p.training_pairs} training comparisons / {p.validation_positive} of {p.validation_pairs} later comparisons rose by more than 10% / {p.coverage_days} observed days.</p><p>Entries match nearby local times. Exits use actual scans within bounded time tolerances. Missing scans remain unknown. Holding ranges come from training observations, not a forecast confidence interval.</p></Details>
    <Details className="strategy-more"><summary>Optional: calculate a purchase</summary>
    <div className="strategy-calculator"><section><h3>Check a purchase</h3><p className="chart-note">Your quantity limit is a manual cap, not an estimated sale volume. Inputs are local to this plan.</p>
      <div className="strategy-inputs"><Label>Gold budget<Input inputMode="decimal" value={budget} onChange={e => setBudget(e.target.value)} placeholder="e.g. 100"/></Label><Label>Maximum units<Input inputMode="numeric" value={maxUnits} onChange={e => setMaxUnits(e.target.value)} placeholder="Your position limit"/></Label><Label>Maximum unit price (g)<Input inputMode="decimal" value={limit} onChange={e => setLimit(e.target.value)}/></Label><Label>Auction fee (%)<Input inputMode="decimal" value={fee} onChange={e => setFee(e.target.value)} placeholder="Confirm the fee"/></Label><Label>Total deposit losses (g)<Input inputMode="decimal" value={deposits} onChange={e => setDeposits(e.target.value)} placeholder="Enter an estimate, or 0"/></Label></div>
      {error ? <Alert className="error-state" role="alert">{error}<Button onClick={() => setRetry(n => n + 1)}>Retry depth</Button></Alert> : !depth ? <p role="status">Loading listing depth…</p> : !depthFresh(depth, now) ? <p className="strategy-note">Fresh, complete listing depth is unavailable. Refresh the market before planning a purchase.</p> : <>
        <dl className="summary-list"><div><dt>Selected whole stacks / units</dt><dd>{plan ? `${plan.stacks.length} / ${plan.units}` : 'Enter budget and limits'}</dd></div><div><dt>Acquisition cost</dt><dd>{plan ? money(plan.cost) : '—'}</dd></div><div><dt>Target scenario profit</dt><dd className={profit != null && profit < 0n ? 'negative' : ''}>{p.target ? money(profit) : 'No target: insufficient evidence'}</dd></div><div><dt>Conservative scenario profit</dt><dd className={conservative != null && conservative < 0n ? 'negative' : ''}>{p.conservative_target ? money(conservative) : 'No target: insufficient evidence'}</dd></div><div><dt>Target scenario ROI</dt><dd>{profit != null && plan && plan.cost > 0n ? `${Number(profit * 10000n / plan.cost) / 100}%` : '—'}</dd></div></dl>
        <p className="chart-note">Cheapest eligible whole stacks first, within both limits. Observed {time(depth.observed_at)}. Scenarios assume every selected unit sells at the reference price. They are not probability-weighted expected profits.</p>
        {!!plan?.stacks.length && <Details><summary>Selected auction stacks</summary><Table><TableHeader><TableRow><TableHead>Units</TableHead><TableHead>Unit price</TableHead><TableHead>Total</TableHead></TableRow></TableHeader><TableBody>{plan.stacks.map((l, i) => <TableRow key={i}><TableCell>{l.stack_count}</TableCell><TableCell><WowMoney value={l.unit_price}/></TableCell><TableCell>{money(BigInt(l.buyout_total_copper))}</TableCell></TableRow>)}</TableBody></Table></Details>}
      </>}
    </section></div></Details>
  </article>;
}
