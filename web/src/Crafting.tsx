import { useEffect, useState } from 'react';
import { api, time, type Price } from './api';
import { Modal } from './Modal';
import { ItemIcon } from './market-ui';
import { craftingMoney, craftingPercent, quoteOptions } from './Crafting.helpers';

type Facts = {
  spell_id: number; profession_id: number; profession_name: string; name: string;
  kind: string; output_item_id: number | null; min_output: number | null; max_output: number | null;
  reagents: Record<string, number>; tools: string[];
};
type Recipe = { output_icon?: string | null; catalog_attribution?: {text: string; url: string} | null; recipe_id: string; build: string; facts: Facts; conflicted: boolean; last_observed_at: string; quote?: Quote | null; status?: string; stale?: boolean; binding_unknown?: boolean; quality?: string };
type Profession = { id: number; name: string; recipe_count: number };
type Quote = {
  status: string; material_cost: Price | null; total_cost: Price | null; cost_per_output: Price | null;
  gross_sale_value: Price | null; net_sale_value: Price | null; estimated_profit: Price | null;
  sale_unit_price: Price | null; output_units: number | null;
  margin_percent: Price | null; return_on_cost_percent: Price | null;
  reagents: { item_id: number; required_units: number; purchased_units: number | null; unit_price: Price | null; cost: Price | null; status: string }[];
};
type Estimate = {
  recipe: Recipe; status: string; quote: Quote | null; snapshot_id?: string; observed_at?: string;
  stale?: boolean; quality?: string; binding_unknown?: boolean; assumptions?: string[];
};
const statuses: Record<string, string> = {
  estimated: 'Estimated from asking prices', missing_materials: 'Missing reagent prices or insufficient supply',
  not_auctionable: 'Service or bound output: material cost only', unknown_or_variable_yield: 'Output yield is unknown or variable',
  missing_sale_price: 'No finished-item asking price in this snapshot', recipe_conflict: 'Recipe observations disagree; profit calculation is paused',
  quote_arithmetic_limit: 'Estimate exceeds calculation limits',
  no_market_snapshot: 'No auction listing snapshot is available for this market',
};

export function Crafting({ market, neutral = false, onItem }: { market: string; neutral?: boolean; onItem: (key: string) => void }) {
  const [professions, setProfessions] = useState<Profession[]>([]), [profession, setProfession] = useState('');
  const [discovery, setDiscovery] = useState({ total_count: 0, unresolved_count: 0 });
  const [sort, setSort] = useState('profit'), [direction, setDirection] = useState('desc');
  const [query, setQuery] = useState(''), [page, setPage] = useState(0), [rows, setRows] = useState<Recipe[]>([]), [total, setTotal] = useState(0);
  const [selected, setSelected] = useState<Recipe | null>(null), [busy, setBusy] = useState(true), [error, setError] = useState(''), [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let active = true;
    Promise.all([api<Profession[]>('/professions'), api<{ total_count: number; unresolved_count: number }>('/recipe-items?limit=1')]).then(([p, d]) => { if (active) { setProfessions(p); setDiscovery(d); } }).catch(e => { if (active) setError(e.message); });
    return () => { active = false; };
  }, [refresh]);
  useEffect(() => {
    let active = true; setBusy(true); setError('');
    if (!market) { setRows([]); setTotal(0); setBusy(false); return; }
    const timer = setTimeout(() => {
      const params = new URLSearchParams({ q: query, limit: '30', offset: String(page * 30), sort, direction });
      if (market) params.set('market_id', market);
      if (profession) params.set('profession_id', profession);
      api<{ recipes: Recipe[]; total_count: number }>(`/recipes?${params}`).then(data => {
        if (active) { setRows(data.recipes); setTotal(data.total_count); }
      }).catch(e => { if (active) setError(e.message); }).finally(() => { if (active) setBusy(false); });
    }, 200);
    return () => { active = false; clearTimeout(timer); };
  }, [query, profession, page, refresh, market, sort, direction]);
  return <section className="crafting" aria-label="Crafting cost simulator">
    <div className="crafting-heading"><div><h1>Crafting</h1><p>Compare production cost with finished-item asking prices.</p></div><button onClick={() => setRefresh(v => v + 1)}>Refresh recipes</button></div>
    <details className="quiet-details"><summary>About these estimates</summary><p>One craft, reference material prices, and a {neutral ? 15 : 5}% auction cut. Sales are not guaranteed.</p><p>Recipes come from shared scans and profession windows. Reload or log out to save discoveries. {discovery.unresolved_count} recipes await details.</p></details>
    <div className="crafting-filters"><label>Recipe search<input type="search" value={query} onChange={e => { setQuery(e.target.value); setPage(0); }} placeholder="Search discovered recipes" /></label><label>Profession<select value={profession} onChange={e => { setProfession(e.target.value); setPage(0); }}><option value="">All professions</option>{professions.map(p => <option key={p.id} value={p.id}>{p.name} ({p.recipe_count})</option>)}</select></label><label>Sort by<select value={sort} onChange={e => { setSort(e.target.value); setDirection(e.target.value === 'profit' ? 'desc' : 'asc'); setPage(0); }}><option value="profit">Estimated profit</option><option value="cost">Craft cost</option><option value="name">Recipe name</option><option value="profession">Profession</option></select></label><label>Order<select value={direction} onChange={e => { setDirection(e.target.value); setPage(0); }}><option value="desc">Highest first / Z-A</option><option value="asc">Lowest first / A-Z</option></select></label></div>
    {error && <p className="error-state" role="alert">{error}</p>}
    <div><div className="table-panel table-scroll"><table className="market-table"><thead><tr><th>Recipe</th><th>Profession</th><th>Output / craft</th><th>Craft cost</th><th>Estimated profit</th><th>Status</th></tr></thead><tbody>
      {!busy && !error && rows.map(r => <tr key={r.recipe_id} aria-selected={selected?.recipe_id === r.recipe_id}><td><button className="crafting-recipe-link" onClick={() => setSelected(r)}>{r.facts.name}</button>{r.conflicted && <small className="crafting-warning"> Conflicting observations</small>}</td><td>{r.facts.profession_name}</td><td>{r.facts.kind === 'service' ? 'Service' : r.facts.min_output == null ? 'Unknown' : r.facts.min_output === r.facts.max_output ? r.facts.min_output : `${r.facts.min_output}–${r.facts.max_output}`}</td><td>{craftingMoney(r.quote?.total_cost)}</td><td>{craftingMoney(r.quote?.estimated_profit)}</td><td>{statuses[r.status ?? ''] ?? r.status}{r.quality && r.quality !== 'complete' && <small className="crafting-warning">{r.quality} coverage</small>}{r.stale && <small className="crafting-warning">Stale snapshot</small>}{r.binding_unknown && <small className="crafting-warning">Binding unverified</small>}</td></tr>)}
    </tbody></table>{busy && <p className="empty-state" role="status">Loading recipes…</p>}{!busy && !rows.length && <div className="empty-state"><h2>No discovered recipes match.</h2><p>Recipes appear after the addon resolves an auction recipe item or reads a profession window, saves the data, and the collector uploads it.</p></div>}</div>
    <div className="table-footer"><span>{total} discovered recipes</span><div className="pagination"><button aria-label="Previous recipe page" disabled={busy || !page} onClick={() => setPage(p => p - 1)}>Previous</button><span>{page + 1}</span><button aria-label="Next recipe page" disabled={busy || rows.length < 30 || (page + 1) * 30 >= total} onClick={() => setPage(p => p + 1)}>Next</button></div></div>
    </div>
    {selected ? <Modal title={selected.facts.name} onClose={() => setSelected(null)}><RecipeEstimate key={`${selected.recipe_id}:${market}:${neutral}:${refresh}`} selected={selected} market={market} neutral={neutral} onItem={key => { setSelected(null); onItem(key); }} /></Modal> : null}
  </section>;
}

function RecipeEstimate({ selected, market, neutral, onItem }: { selected: Recipe; market: string; neutral: boolean; onItem: (key: string) => void }) {
  const [crafts, setCrafts] = useState('1'), [fee, setFee] = useState(neutral ? '15' : '5'), [extra, setExtra] = useState('0'), [mode, setMode] = useState('reference');
  const [result, setResult] = useState<Estimate | null>(null), [busy, setBusy] = useState(false), [error, setError] = useState('');
  const options = quoteOptions(crafts, fee, extra, mode), valid = options !== null;
  const params = options?.toString() ?? '';
  useEffect(() => {
    let active = true; setResult(null); setError('');
    if (!market || !valid) { setBusy(false); return; }
    setBusy(true);
    const timer = setTimeout(() => {
      api<Estimate>(`/recipes/${selected.facts.spell_id}/quote?${params}&market_id=${encodeURIComponent(market)}&build=${encodeURIComponent(selected.build)}`).then(data => { if (active) setResult(data); }).catch(e => { if (active) setError(e.message); }).finally(() => { if (active) setBusy(false); });
    }, 200);
    return () => { active = false; clearTimeout(timer); };
  }, [market, valid, params, selected.facts.spell_id, selected.build]);
  const facts = result?.recipe.facts ?? selected.facts, quote = result?.quote;
  return <aside className="crafting-detail" aria-label="Recipe estimate"><div className="detail-identity recipe-identity"><ItemIcon name={facts.name} icon={result?.recipe.output_icon ?? selected.output_icon} large/><div><h2>{facts.name}</h2><p className="muted">{facts.profession_name}</p></div></div>
    {quote && <dl className="crafting-summary"><div><dt>Batch cost</dt><dd>{craftingMoney(quote.total_cost)}</dd></div><div><dt>Sale after cut</dt><dd>{craftingMoney(quote.net_sale_value)}</dd></div><div><dt>Estimated profit</dt><dd>{craftingMoney(quote.estimated_profit)}</dd></div></dl>}
    <div className="crafting-controls"><label>Crafts<input type="number" min="1" max="10000" step="1" value={crafts} onChange={e => setCrafts(e.target.value)} /></label><label>Auction cut (%)<input type="number" min="0" max="100" step="0.01" value={fee} onChange={e => setFee(e.target.value)} /></label><label>Additional batch cost (copper)<input type="text" inputMode="numeric" value={extra} onChange={e => setExtra(e.target.value)} /></label><label>Material pricing<select value={mode} onChange={e => setMode(e.target.value)}><option value="reference">Reference unit prices</option><option value="purchase">Buy whole auction stacks</option></select></label></div>
    {!valid && <p role="alert">Enter 1–10,000 crafts, a cut from 0 to 100%, and a whole copper cost.</p>}
    {!market && <p>Waiting for an accepted scan from Normal · Horde.</p>}{busy && <p role="status">Calculating…</p>}{error && <p role="alert" className="error-state">{error}</p>}
    {result && <>{result.status !== 'estimated' && <p className={result.status === 'estimated' ? 'muted' : 'crafting-warning'}>{statuses[result.status] ?? result.status}</p>}{result.stale && <p className="crafting-warning">This market snapshot is more than one hour old.</p>}{result.binding_unknown && <p className="crafting-warning">Output binding has not been verified.</p>}</>}
    <h3>Reagents</h3><table className="crafting-reagents"><thead><tr><th>Item</th><th>Required</th><th>Cost</th></tr></thead><tbody>{Object.entries(facts.reagents).map(([id, quantity]) => {
      const line = quote?.reagents.find(r => r.item_id === Number(id));
      return <tr key={id}><td><button onClick={() => onItem(`i${id}`)}>Item {id}</button>{line && line.status !== 'priced' && <small className="crafting-warning">{line.status === 'insufficient_supply' ? 'Insufficient supply' : 'No price'}</small>}</td><td>{line?.required_units ?? quantity}<small>{line?.purchased_units != null ? `${line.purchased_units} units available for purchase${line.status === 'priced' ? ' / bought' : ''}` : !quote ? 'per craft' : ''}</small></td><td>{craftingMoney(line?.cost)}</td></tr>;
    })}</tbody></table>
    {facts.output_item_id && <p>Output: <button onClick={() => onItem(`i${facts.output_item_id}`)}>Item {facts.output_item_id}</button>{quote?.output_units != null && ` · ${quote.output_units} units per batch`}</p>}
    {facts.tools.length > 0 && <p>Tools / station: {facts.tools.join(', ')}</p>}
    <details className="quiet-details"><summary>Cost breakdown and assumptions</summary>{quote && <dl className="crafting-totals"><dt>Material cost</dt><dd>{craftingMoney(quote.material_cost)}</dd><dt>Total batch cost</dt><dd>{craftingMoney(quote.total_cost)}</dd><dt>Cost per output</dt><dd>{craftingMoney(quote.cost_per_output)}</dd><dt>Finished-item unit ask</dt><dd>{craftingMoney(quote.sale_unit_price)}</dd><dt>Gross sale value</dt><dd>{craftingMoney(quote.gross_sale_value)}</dd><dt>Sale value after cut</dt><dd>{craftingMoney(quote.net_sale_value)}</dd><dt>Estimated batch profit</dt><dd className="crafting-profit">{craftingMoney(quote.estimated_profit)}</dd><dt>Net margin</dt><dd>{craftingPercent(quote.margin_percent)}</dd><dt>Return on cost</dt><dd>{craftingPercent(quote.return_on_cost_percent)}</dd></dl>}
    <p className="muted">Asking prices do not prove a sale. Whole-stack mode includes the cash cost of surplus units; it does not find the cheapest possible combination of stacks.</p>
    <p className="muted">Updated {time(result?.observed_at ?? selected.last_observed_at)}</p>{selected.catalog_attribution && <a href={selected.catalog_attribution.url} target="_blank" rel="noreferrer">{selected.catalog_attribution.text}</a>}
    {result?.assumptions && <ul className="crafting-assumptions">{result.assumptions.map(a => <li key={a}>{a}</li>)}</ul>}
    </details>
  </aside>;
}
