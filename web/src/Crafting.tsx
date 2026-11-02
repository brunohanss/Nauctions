import { WowMoney as CoinMoney } from './WowMoney';
import { ItemTooltip } from './ItemTooltip';
import { merchantReasons, type MerchantQuote } from "./MerchantProfit";
import { ArrowDown, ArrowUp } from "lucide-react";
import { ResultsToolbar, LoadingRows } from "./components/results";
import { Empty } from "./components/ui/empty";
import { Alert } from "./components/ui/alert";
import {
  Pager,
  Button,
  Input,
  Label,
  Select,
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
  Details,
} from "./components/controls";
import { useEffect, useState } from "react";
import { api, time, type Price } from "./api";
import { Modal } from "./Modal";
import { ItemIcon } from "./market-ui";
import { quoteOptions, undercutEstimate, profitPercent } from "./Crafting.helpers";
import { Percentage } from "./Percentage";
import { buildPurchasePlan, type PurchasePlan, type PlanSource } from "./Crafting.plan";
import { TableFooter } from "./components/ui/table";

type Facts = {
  spell_id: number;
  profession_id: number;
  profession_name: string;
  name: string;
  kind: string;
  output_item_id: number | null;
  min_output: number | null;
  max_output: number | null;
  reagents: Record<string, number>;
  tools: string[];
};
type Recipe = {
  output_icon?: string | null;
  catalog_attribution?: { text: string; url: string } | null;
  recipe_id: string;
  build: string;
  facts: Facts;
  conflicted: boolean;
  last_observed_at: string;
  quote?: Quote | null;
  status?: string;
  stale?: boolean;
  binding_unknown?: boolean;
  quality?: string;
  merchant?: MerchantQuote | null;
};
type Profession = { id: number; name: string; recipe_count: number };
type Quote = {
  status: string;
  material_cost: Price | null;
  total_cost: Price | null;
  cost_per_output: Price | null;
  gross_sale_value: Price | null;
  net_sale_value: Price | null;
  estimated_profit: Price | null;
  sale_unit_price: Price | null;
  output_units: number | null;
  margin_percent: Price | null;
  return_on_cost_percent: Price | null;
  reagents: {
    item_id: number;
    required_units: number;
    purchased_units: number | null;
    unit_price: Price | null;
    cost: Price | null;
    status: string;
  }[];
};
type Estimate = {
  recipe: Recipe;
  status: string;
  quote: Quote | null;
  merchant?: MerchantQuote | null;
  snapshot_id?: string;
  observed_at?: string;
  stale?: boolean;
  quality?: string;
  binding_unknown?: boolean;
  assumptions?: string[];
};
const statuses: Record<string, string> = {
  estimated: "Estimated from asking prices",
  missing_materials: "Missing reagent prices or insufficient supply",
  not_auctionable: "Service or bound output: material cost only",
  unknown_or_variable_yield: "Output yield is unknown or variable",
  missing_sale_price: "No finished-item asking price in this snapshot",
  recipe_conflict: "Recipe observations disagree; profit calculation is paused",
  quote_arithmetic_limit: "Estimate exceeds calculation limits",
  no_market_snapshot:
    "No auction listing snapshot is available for this market",
};

export function Crafting({
  market,
  neutral = false,
  onItem,
}: {
  market: string;
  neutral?: boolean;
  onItem: (key: string) => void;
}) {
  const [professions, setProfessions] = useState<Profession[]>([]),
    [profession, setProfession] = useState("");
  const [discovery, setDiscovery] = useState({
    total_count: 0,
    unresolved_count: 0,
  });
  const [sort, setSort] = useState("profit"),
    [direction, setDirection] = useState("desc");
  const [merchantOnly, setMerchantOnly] = useState(false);
  const [query, setQuery] = useState(""),
    [page, setPage] = useState(0),
    [rows, setRows] = useState<Recipe[]>([]),
    [total, setTotal] = useState(0);
  const [selected, setSelected] = useState<Recipe | null>(null),
    [busy, setBusy] = useState(true),
    [error, setError] = useState(""),
    [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let active = true;
    Promise.all([
      api<Profession[]>("/professions"),
      api<{ total_count: number; unresolved_count: number }>(
        "/recipe-items?limit=1",
      ),
    ])
      .then(([p, d]) => {
        if (active) {
          setProfessions(p);
          setDiscovery(d);
        }
      })
      .catch((e) => {
        if (active) setError(e.message);
      });
    return () => {
      active = false;
    };
  }, [refresh]);
  useEffect(() => {
    let active = true;
    setBusy(true);
    setError("");
    if (!market) {
      setRows([]);
      setTotal(0);
      setBusy(false);
      return;
    }
    const timer = setTimeout(() => {
      const params = new URLSearchParams({
        q: query,
        limit: "30",
        offset: String(page * 30),
        sort,
        direction,
      });
      if (market) params.set("market_id", market);
      if (merchantOnly) params.set("merchant_profitable", "true");
      if (profession) params.set("profession_id", profession);
      api<{ recipes: Recipe[]; total_count: number }>(`/recipes?${params}`)
        .then((data) => {
          if (active) {
            setRows(data.recipes);
            setTotal(data.total_count);
          }
        })
        .catch((e) => {
          if (active) setError(e.message);
        })
        .finally(() => {
          if (active) setBusy(false);
        });
    }, 200);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [query, profession, page, refresh, market, sort, direction, merchantOnly]);
  function changeSort(key: string) {
    setSort(key);
    setDirection(
      sort === key
        ? direction === "desc"
          ? "asc"
          : "desc"
        : ["profit", "merchant_profit"].includes(key)
          ? "desc"
          : "asc",
    );
    setPage(0);
  }
  function heading(label: string, key: string, numeric = false) {
    return (
      <TableHead
        className={numeric ? "numeric" : undefined}
        aria-sort={
          sort === key
            ? direction === "asc"
              ? "ascending"
              : "descending"
            : undefined
        }
      >
        <Button variant="ghost" onClick={() => changeSort(key)}>
          {label}
          {sort === key &&
            (direction === "asc" ? (
              <ArrowUp size={12} />
            ) : (
              <ArrowDown size={12} />
            ))}
        </Button>
      </TableHead>
    );
  }
  return (
    <section className="crafting" aria-label="Crafting cost simulator">
      <div className="crafting-heading">
        <div>
          <h1>Crafting</h1>
          <p>Compare production cost with finished-item asking prices.</p>
        </div>
        <Button onClick={() => setRefresh((v) => v + 1)}>
          Refresh recipes
        </Button>
      </div>
      <Details className="quiet-details">
        <summary>About these estimates</summary>
        <p>
          One craft, reference material prices, and a {neutral ? 15 : 5}% cut on
          auction sales. Merchant profit uses whole-stack reagent purchases and
          no auction cut. Auction sales are not guaranteed.
        </p>
        <p>
          Recipes come from shared scans and profession windows. Reload or log
          out to save discoveries. {discovery.unresolved_count} recipes await
          details.
        </p>
      </Details>
      <ResultsToolbar>
        <Label>
          Recipe search
          <Input
            type="search"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setPage(0);
            }}
            placeholder="Search discovered recipes"
          />
        </Label>
        <Label>
          Profession
          <Select
            value={profession}
            onChange={(e) => {
              setProfession(e.target.value);
              setPage(0);
            }}
          >
            <option value="">All professions</option>
            {professions.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} ({p.recipe_count})
              </option>
            ))}
          </Select>
        </Label>
        <Label>
          Sort by
          <Select
            value={sort}
            onChange={(e) => {
              setSort(e.target.value);
              setDirection(
                ["profit", "merchant_profit"].includes(e.target.value)
                  ? "desc"
                  : "asc",
              );
              setPage(0);
            }}
          >
            <option value="profit">Auction profit</option>
            <option value="merchant_profit">Merchant profit</option>
            <option value="output">Output per craft</option>
            <option value="cost">Craft cost</option>
            <option value="name">Recipe name</option>
            <option value="profession">Profession</option>
          </Select>
        </Label>
        <Label>
          Order
          <Select
            value={direction}
            onChange={(e) => {
              setDirection(e.target.value);
              setPage(0);
            }}
          >
            <option value="desc">Highest first / Z-A</option>
            <option value="asc">Lowest first / A-Z</option>
          </Select>
        </Label>
        <Label>
          <Input
            type="checkbox"
            checked={merchantOnly}
            onChange={(e) => {
              setMerchantOnly(e.target.checked);
              setPage(0);
            }}
          />
          Merchant profit &gt; 0
        </Label>
      </ResultsToolbar>
      {error && (
        <Alert className="error-state" role="alert">
          {error}
        </Alert>
      )}
      <div>
        <div className="table-panel table-scroll">
          <Table className="market-table">
            <TableHeader>
              <TableRow>
                {heading("Recipe", "name")}
                {heading("Profession", "profession")}
                {heading("Output / craft", "output", true)}
                {heading("Craft cost", "cost", true)}
                {heading("Auction profit", "profit", true)}
                {heading("Merchant profit", "merchant_profit", true)}
              </TableRow>
            </TableHeader>
            <TableBody>
              {busy && <LoadingRows columns={6} />}
              {!busy &&
                !error &&
                rows.map((r) => (
                  <TableRow
                    key={r.recipe_id}
                    aria-selected={selected?.recipe_id === r.recipe_id}
                  >
                    <TableCell>
                      <ItemTooltip name={r.facts.name} itemId={r.facts.output_item_id}><div className="item-cell" tabIndex={0}>
                        <ItemIcon name={r.facts.name} icon={r.output_icon} />
                        <Button
                          data-item-name
                          className="crafting-recipe-link"
                          onClick={() => setSelected(r)}
                        >
                          {r.facts.name}
                        </Button>
                      </div></ItemTooltip>
                      {r.quality && r.quality !== "complete" && (
                        <small className="crafting-warning">
                          {r.quality} coverage
                        </small>
                      )}
                      {r.stale && (
                        <small className="crafting-warning">
                          Stale snapshot
                        </small>
                      )}
                      {r.binding_unknown && (
                        <small className="crafting-warning">
                          Binding unverified
                        </small>
                      )}
                      {r.conflicted && (
                        <small className="crafting-warning">
                          {" "}
                          Conflicting observations
                        </small>
                      )}
                    </TableCell>
                    <TableCell>{r.facts.profession_name}</TableCell>
                    <TableCell>
                      {r.facts.kind === "service"
                        ? "Service"
                        : r.facts.min_output == null
                          ? "Unknown"
                          : r.facts.min_output === r.facts.max_output
                            ? r.facts.min_output
                            : `${r.facts.min_output}–${r.facts.max_output}`}
                    </TableCell>
                    <TableCell className="numeric">
                      <CoinMoney value={r.quote?.total_cost} />
                    </TableCell>
                    <TableCell className="numeric">
                      <div className="crafting-profit-value"><CoinMoney value={r.quote?.estimated_profit} /><span title="Profit / material cost"><Percentage value={profitPercent(r.quote?.estimated_profit, r.quote?.total_cost)} baseline={0} signed /></span></div>
                    </TableCell>
                    <TableCell className="numeric">
                      <div className="crafting-profit-value"><CoinMoney value={r.merchant?.profit} /><span title="Profit / reagent purchase cost"><Percentage value={profitPercent(r.merchant?.profit, r.merchant?.purchase_cost)} baseline={0} signed /></span></div>
                      {r.merchant?.status && r.merchant.status !== "estimated" && <small className="muted">{merchantReasons[r.merchant.status] ?? "Merchant estimate unavailable"}</small>}
                    </TableCell>
                  </TableRow>
                ))}
            </TableBody>
          </Table>
          {busy && (
            <Empty className="empty-state" role="status">
              Loading recipes…
            </Empty>
          )}
          {!busy && !rows.length && (
            <Empty className="empty-state">
              <h2>No discovered recipes match.</h2>
              <p>
                Recipes appear after the addon resolves an auction recipe item
                or reads a profession window, saves the data, and the collector
                uploads it.
              </p>
            </Empty>
          )}
        </div>
        <div className="table-footer">
          <span>{total} discovered recipes</span>
          <Pager>
            <Button
              aria-label="Previous recipe page"
              disabled={busy || !page}
              onClick={() => setPage((p) => p - 1)}
            >
              Previous
            </Button>
            <span>{page + 1}</span>
            <Button
              aria-label="Next recipe page"
              disabled={busy || rows.length < 30 || (page + 1) * 30 >= total}
              onClick={() => setPage((p) => p + 1)}
            >
              Next
            </Button>
          </Pager>
        </div>
      </div>
      {selected ? (
        <Modal title={selected.facts.name} onClose={() => setSelected(null)}>
          <RecipeEstimate
            key={`${selected.recipe_id}:${market}:${neutral}:${refresh}`}
            selected={selected}
            market={market}
            onItem={(key) => {
              setSelected(null);
              onItem(key);
            }}
          />
        </Modal>
      ) : null}
    </section>
  );
}

type IngredientMetadata = { name: string; quality?: number };
const ingredientMetadata = import.meta.glob<IngredientMetadata>(
  "../../assets/item-icon-metadata/*.json", { eager: true, import: "default" },
);

function IngredientItem({ id, onItem }: { id: string; onItem: (key: string) => void }) {
  const local = ingredientMetadata[`../../assets/item-icon-metadata/${id}.json`];
  const [metadata, setMetadata] = useState<IngredientMetadata | undefined>(local);
  useEffect(() => {
    if (local) return;
    let active = true;
    api<IngredientMetadata>(`/item-metadata/${id}`).then(
      data => { if (active) setMetadata(data); },
      () => {},
    );
    return () => { active = false; };
  }, [id, local]);
  const name = metadata?.name ?? `Item ${id}`;
  return (
    <ItemTooltip name={name} itemId={id} rarity={metadata?.quality}>
      <div className="item-cell" tabIndex={0}>
        <ItemIcon name={name} icon={`/item-icons/${id}.jpg`} />
        <span className="item-name">
          <Button variant="link" className="item-link ingredient-link" data-item-name onClick={() => onItem(`i${id}`)}>{name}</Button>
        </span>
      </div>
    </ItemTooltip>
  );
}

function RecipeEstimate({
  selected,
  market,
  onItem,
}: {
  selected: Recipe;
  market: string;
  onItem: (key: string) => void;
}) {
  const [crafts, setCrafts] = useState("1");
  const [result, setResult] = useState<Estimate | null>(null),
    [busy, setBusy] = useState(true),
    [error, setError] = useState("");
  const options = quoteOptions(crafts, "0", "0", "reference"),
    valid = options !== null;
  const params = options?.toString() ?? "";
  useEffect(() => {
    let active = true;
    setResult(null);
    setError("");
    if (!market || !valid) {
      setBusy(false);
      return;
    }
    setBusy(true);
    const timer = setTimeout(() => {
      api<Estimate>(
        `/recipes/${selected.facts.spell_id}/quote?${params}&market_id=${encodeURIComponent(market)}&build=${encodeURIComponent(selected.build)}`,
      )
        .then((data) => {
          if (active) setResult(data);
        })
        .catch((e) => {
          if (active) setError(e.message);
        })
        .finally(() => {
          if (active) setBusy(false);
        });
    }, 200);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [market, valid, params, selected.facts.spell_id, selected.build]);
  const facts = result?.recipe.facts ?? selected.facts,
    quote = result?.quote;
  const [plan, setPlan] = useState<PurchasePlan | null>(null);
  const [planError, setPlanError] = useState("");
  const [planBusy, setPlanBusy] = useState(false);
  useEffect(() => {
    let active = true;
    setPlan(null);
    setPlanError("");
    if (!result?.quote || !valid) { setPlanBusy(false); return; }
    setPlanBusy(true);
    const prices = Object.fromEntries(result.quote.reagents.map(line => [String(line.item_id), line.unit_price]));
    const load = async (id: string): Promise<PlanSource> => {
      const candidates = await api<{ recipes: Recipe[]; total_count: number }>(`/recipes?${new URLSearchParams({
        output_item_id: id, build: selected.build, market_id: market, limit: "2", sort: "name", direction: "asc",
      })}`);
      if (!candidates.total_count) return {};
      if (candidates.total_count !== 1) return { reason: "Multiple recipes found. Buy this component directly." };
      const candidate = candidates.recipes[0];
      const f = candidate.facts;
      if (candidate.conflicted || candidate.build !== selected.build || String(f.output_item_id) !== id || f.kind !== "item" || !f.min_output || f.min_output !== f.max_output || !Object.keys(f.reagents).length)
        return { reason: "No confirmed fixed-yield recipe. Buy this component directly." };
      const data = await api<Estimate>(`/recipes/${f.spell_id}/quote?${new URLSearchParams({
        market_id: market, build: selected.build, crafts: "1", mode: "reference", fee_bps: "0",
      })}`);
      if (data.snapshot_id !== result.snapshot_id) throw new Error("The market scan changed. Close and reopen the recipe to refresh the purchase list.");
      if (!data.quote || data.recipe.conflicted) return { reason: "Recipe details are unavailable. Buy this component directly." };
      return {
        recipe: { name: f.name, output: f.min_output, reagents: f.reagents },
        prices: Object.fromEntries(data.quote.reagents.map(line => [String(line.item_id), line.unit_price])),
      };
    };
    buildPurchasePlan(result.recipe.facts.reagents, Number(crafts), prices, load)
      .then(data => { if (active) setPlan(data); })
      .catch(error => { if (active) setPlanError(error.message); })
      .finally(() => { if (active) setPlanBusy(false); });
    return () => { active = false; };
  }, [result, crafts, valid, market, selected.build]);
  const cost = plan ? plan.total : (planBusy || planError ? null : quote?.material_cost);
  const sale = undercutEstimate(quote ? { ...quote, material_cost: cost ?? null } : null);
  const purchaseRows = plan?.purchases ?? Object.entries(facts.reagents).map(([id, quantity]) => {
    const line = quote?.reagents.find(row => row.item_id === Number(id));
    return { id, units: valid ? quantity * Number(crafts) : null, unitPrice: line?.unit_price, cost: line?.cost, reason: undefined };
  });
  return (
    <aside className="crafting-detail" aria-label="Crafting ingredients">
      <ItemTooltip name={facts.name} itemId={facts.output_item_id}><div className="detail-identity recipe-identity">
        <ItemIcon
          name={facts.name}
          icon={result?.recipe.output_icon ?? selected.output_icon}
          large
        />
        <div>
          <h2 data-item-name>{facts.name}</h2>
          <p className="muted">{facts.profession_name}</p>
        </div>
      </div></ItemTooltip>
      <div className="crafting-batch">
        <Label htmlFor="crafting-quantity">Number of crafts</Label>
        <Input
          id="crafting-quantity"
          type="number"
          min="1"
          max="10000"
          step="1"
          inputMode="numeric"
          value={crafts}
          aria-invalid={!valid}
          aria-describedby="crafting-quantity-help"
          onChange={(e) => { setCrafts(e.target.value); setResult(null); }}
        />
        <p id="crafting-quantity-help" className="muted">Ingredient quantities and costs apply to the full batch.</p>
      </div>
      {!valid && <p role="alert">Enter a whole number from 1 to 10,000.</p>}
      {!market && <p>Waiting for a market scan.</p>}
      {busy && <p role="status">Loading ingredient prices…</p>}
      {error && <Alert role="alert" className="error-state">{error}</Alert>}
      <h3 className="crafting-ingredients-heading">Recipe components</h3>
      <div className="table-panel table-scroll">
        <Table className="market-table crafting-reagents" aria-label="Recipe components">
          <TableHeader><TableRow><TableHead>Component</TableHead><TableHead className="numeric">Required</TableHead></TableRow></TableHeader>
          <TableBody>{Object.entries(facts.reagents).map(([id, quantity]) => (
            <TableRow key={id}>
              <TableCell><IngredientItem id={id} onItem={onItem} /></TableCell>
              <TableCell className="numeric">{valid ? quantity * Number(crafts) : "—"}</TableCell>
            </TableRow>
          ))}</TableBody>
        </Table>
      </div>
      <h3 className="crafting-ingredients-heading crafting-purchases-heading">Materials to buy</h3>
      <p className="chart-note">Craft intermediate components from the materials below. Prices use the latest scan.</p>
      {planBusy && <p role="status">Expanding intermediate recipes…</p>}
      {planError && <Alert role="alert">{planError}</Alert>}
      <div className="table-panel table-scroll">
      <Table className="market-table crafting-reagents" aria-label="Required ingredients" aria-busy={busy || planBusy}>
        <TableHeader>
          <TableRow>
            <TableHead>Item</TableHead>
            <TableHead className="numeric">Buy quantity</TableHead>
            <TableHead className="numeric">Unit price</TableHead>
            <TableHead className="numeric">Total price</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {!planBusy && !planError && purchaseRows.map(row => (
            <TableRow key={row.id}>
              <TableCell>
                <IngredientItem id={row.id} onItem={onItem} />
                {row.reason && <small className="crafting-warning">{row.reason}</small>}
                {quote && !row.unitPrice && <small className="crafting-warning">No price</small>}
              </TableCell>
              <TableCell className="numeric">{row.units ?? "—"}</TableCell>
              <TableCell className="numeric"><CoinMoney value={row.unitPrice} /></TableCell>
              <TableCell className="numeric"><CoinMoney value={row.cost} /></TableCell>
            </TableRow>
          ))}
        </TableBody>
        <TableFooter>
          <TableRow>
            <TableCell colSpan={3}>Total material cost</TableCell>
            <TableCell className="numeric"><CoinMoney value={cost} /></TableCell>
          </TableRow>
          <TableRow>
            <TableCell colSpan={3}>Latest lowest asking price / item</TableCell>
            <TableCell className="numeric"><CoinMoney value={quote?.sale_unit_price} /></TableCell>
          </TableRow>
          <TableRow>
            <TableCell colSpan={3}>Selling price / item (1 copper below)</TableCell>
            <TableCell className="numeric"><CoinMoney value={sale.unitPrice} /></TableCell>
          </TableRow>
          <TableRow>
            <TableCell colSpan={3}>Batch sale value</TableCell>
            <TableCell className="numeric"><CoinMoney value={sale.revenue} /></TableCell>
          </TableRow>
          <TableRow>
            <TableCell colSpan={3}>Estimated batch profit</TableCell>
            <TableCell className={`numeric ${sale.profit ? Number(sale.profit.numerator) < 0 ? "negative" : "positive" : ""}`}>
              <div className="crafting-profit-value"><CoinMoney value={sale.profit} /><Percentage value={profitPercent(sale.profit, cost)} baseline={0} signed /></div>
            </TableCell>
          </TableRow>
        </TableFooter>
      </Table>
      </div>
      {plan && plan.steps.length > 0 && (
        <section className="crafting-intermediates" aria-label="Intermediate crafts">
          <h3>Craft these components first</h3>
          <ol>{plan.steps.map(step => (
            <li key={step.id}><IngredientItem id={step.id} onItem={onItem} /><span>{step.crafts} × {step.name} · {step.required} needed{step.surplus > 0 ? ` · ${step.surplus} surplus` : ""}</span></li>
          ))}</ol>
        </section>
      )}
      {result && !busy && !planBusy && !error && !planError && sale.profit == null && (
        <p className="crafting-warning">{plan && plan.total == null
          ? "Purchase prices are incomplete. Total cost and profit are unavailable."
          : result.status === "estimated"
            ? "A selling price above 1 copper and complete material prices are required to estimate profit."
            : statuses[result.status] ?? "Ingredient prices are incomplete. Total cost is unavailable."}</p>
      )}
      {result?.stale && <p className="crafting-warning">This market snapshot is more than one hour old.</p>}
      {result?.binding_unknown && <p className="crafting-warning">Output binding has not been verified.</p>}
      <p className="muted">Profit uses a selling price of 1 copper below the latest lowest asking price, minus material cost. No percentage deduction.</p>
      {facts.output_item_id && (
        <p>
          Output: <Button onClick={() => onItem(`i${facts.output_item_id}`)}>{facts.name}</Button>
          {quote?.output_units != null && ` · ${quote.output_units} units per batch`}
        </p>
      )}
      {facts.tools.length > 0 && <p>Tools / station: {facts.tools.join(", ")}</p>}
      <p className="muted">Updated {time(result?.observed_at ?? selected.last_observed_at)}</p>
      {selected.catalog_attribution && (
        <a href={selected.catalog_attribution.url} target="_blank" rel="noreferrer">{selected.catalog_attribution.text}</a>
      )}
    </aside>
  );
}
