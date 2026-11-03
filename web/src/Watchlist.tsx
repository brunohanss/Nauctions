import { useCallback, useEffect, useState } from "react";
import { api, ApiError, time, type Item, type Price } from "./api";
import { useMarketRefresh } from "./marketClock";
import {
  WowMoney,
  FreshnessIndicator,
  ItemIcon,
  DiscountBadge,
} from "./market-ui";
import {
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
} from "./components/controls";
import { Alert } from "./components/ui/alert";
import { useWatchlist, WatchlistButton } from "./WatchlistState";
import { TestAlertButton } from "./TestAlertButton";

type Rule = {
  id: string;
  item_key: string;
  kind: string;
  threshold_copper: number | string | null;
  discount_percent: number;
  enabled: boolean;
};
type Event = {
  id: string;
  item_key: string;
  observed_at: string;
  evidence: {
    market_id?: string;
    current_price?: Price;
    signal?: { kind?: string };
  };
  status: string;
};
const ruleName = (kind: string) =>
  kind === "threshold"
    ? "Price target"
    : kind === "underpriced"
      ? "Discount"
      : "Market movement";

export function Watchlist({
  market,
  onItem,
}: {
  market: string;
  onItem: (key: string) => void;
}) {
  const watch = useWatchlist()!;
  const refresh = useMarketRefresh();
  const [page, setPage] = useState(0),
    [selected, setSelected] = useState("");
  const [rules, setRules] = useState<Rule[]>([]),
    [events, setEvents] = useState<Event[]>([]);
  const [error, setError] = useState(""),
    [loading, setLoading] = useState(true),
    [pending, setPending] = useState(false),
    [retry, setRetry] = useState(0);
  const [kind, setKind] = useState("threshold"),
    [value, setValue] = useState("");
  const [coins, setCoins] = useState({ gold: "", silver: "", copper: "" });
  const [names, setNames] = useState<Record<string, string>>({});
  const rememberName = useCallback(
    (key: string, name: string) =>
      setNames((current) =>
        current[key] === name ? current : { ...current, [key]: name },
      ),
    [],
  );
  useEffect(() => {
    let active = true;
    Promise.all([api<Rule[]>("/alert-rules"), api<Event[]>("/alerts")])
      .then(([r, e]) => {
        if (active) {
          setRules(r);
          setEvents(e);
          setError("");
        }
      })
      .catch((e) => {
        if (active) setError(e.message);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [refresh, retry]);
  async function changeRule(path: string, method: string, body?: unknown) {
    if (pending) return false;
    setPending(true);
    setError("");
    try {
      await api(path, method, body);
      setRetry((n) => n + 1);
      return true;
    } catch (e) {
      setError((e as Error).message);
      return false;
    } finally {
      setPending(false);
    }
  }
  async function addRule(e: React.FormEvent) {
    e.preventDefault();
    if (!selected) return;
    const validCoins =
      Object.values(coins).every((part) => /^\d*$/.test(part)) &&
      Number(coins.silver) <= 99 &&
      Number(coins.copper) <= 99;
    const copper =
      kind === "threshold" && validCoins
        ? BigInt(coins.gold || "0") * 10000n +
          BigInt(coins.silver || "0") * 100n +
          BigInt(coins.copper || "0")
        : null;
    const percent = Number(value);
    if (
      kind === "threshold"
        ? copper === null ||
          copper <= 0n ||
          copper > BigInt(Number.MAX_SAFE_INTEGER)
        : !/^\d+$/.test(value) || percent < 1 || percent > 99
    ) {
      setError(
        kind === "threshold"
          ? "Enter a positive price in whole gold, silver, and copper. Silver and copper must be from 0 to 99."
          : "Enter a whole discount percentage from 1 to 99.",
      );
      return;
    }
    // Rules record in-app events. Native notification delivery is a separate feature.
    if (
      await changeRule("/alert-rules", "POST", {
        item_key: selected,
        kind,
        ...(copper !== null
          ? { threshold_copper: Number(copper) }
          : { discount_percent: percent }),
        enabled: true,
        notify: false,
      })
    ) {
      setSelected("");
      setValue("");
      setCoins({ gold: "", silver: "", copper: "" });
    }
  }
  const lastPage = Math.max(0, Math.ceil(watch.keys.length / 10) - 1),
    currentPage = Math.min(page, lastPage);
  const visible = watch.keys.slice(currentPage * 10, currentPage * 10 + 10);
  const marketEvents = events.filter(
    (event) => event.evidence.market_id === market,
  );
  return (
    <section className="watchlist-page">
      <div>
        <h1>Watchlist</h1>
        <p>
          Your saved items and personal alert rules. Prices use the selected
          market.
        </p>
      </div>
      {watch.error && (
        <Alert role="alert">
          Could not load or update your watchlist. {watch.error}{" "}
          <Button onClick={watch.reload}>Retry watchlist</Button>
        </Alert>
      )}
      {watch.loading ? (
        <p role="status">Loading watchlist…</p>
      ) : !watch.error && !watch.keys.length ? (
        <div className="empty">
          <h2>No watched items yet</h2>
          <p>
            Open an item from Market, Crafting, or Strategy, then select “Add to
            watchlist”.
          </p>
        </div>
      ) : (
        <>
          <div className="table-scroll">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Item</TableHead>
                  <TableHead>Minimum / unit</TableHead>
                  <TableHead>Vs. 7d reference</TableHead>
                  <TableHead>Last observed</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {visible.map((key) => (
                  <WatchedItem
                    key={`${market}:${key}`}
                    itemKey={key}
                    market={market}
                    refresh={refresh}
                    onItem={onItem}
                    onName={rememberName}
                    onRule={() => {
                      setSelected(key);
                      setValue("");
                      setCoins({ gold: "", silver: "", copper: "" });
                    }}
                  />
                ))}
              </TableBody>
            </Table>
          </div>
          {lastPage > 0 && (
            <div className="watchlist-pagination">
              <Button
                disabled={currentPage === 0}
                onClick={() => setPage(currentPage - 1)}
              >
                Previous
              </Button>
              <span>
                Page {currentPage + 1} of {lastPage + 1}
              </span>
              <Button
                disabled={currentPage === lastPage}
                onClick={() => setPage(currentPage + 1)}
              >
                Next
              </Button>
            </div>
          )}
        </>
      )}
      <section className="watchlist-section" aria-labelledby="watch-rules">
        <h2 id="watch-rules">Personal alert rules</h2>
        <TestAlertButton/>
        <p>
          Rules check new eligible scans and record matches in the alert history
          below. Automatic Windows alerts are not connected yet. Removing an item
          from the watchlist keeps its rules.
        </p>
        {selected && (
          <form className="watchlist-rule-form" onSubmit={addRule}>
            <p>
              New rule for <strong>{names[selected] ?? selected}</strong>
            </p>
            <Label htmlFor="watch-rule-kind">Condition</Label>
            <Select
              id="watch-rule-kind"
              value={kind}
              onChange={(e) => {
                setKind(e.target.value);
                setValue("");
                setCoins({ gold: "", silver: "", copper: "" });
              }}
            >
              <option value="threshold">Price at or below target</option>
              <option value="underpriced">Discount from 7d reference</option>
            </Select>
            {kind === "threshold" ? (
              <fieldset className="watchlist-coins">
                <legend>Target unit price</legend>
                {(["gold", "silver", "copper"] as const).map((coin) => (
                  <div key={coin}>
                    <Label htmlFor={`watch-rule-${coin}`}>
                      {coin[0].toUpperCase() + coin.slice(1)}
                    </Label>
                    <Input
                      id={`watch-rule-${coin}`}
                      type="number"
                      min="0"
                      max={coin === "gold" ? undefined : 99}
                      step="1"
                      inputMode="numeric"
                      placeholder="0"
                      value={coins[coin]}
                      onChange={(e) =>
                        setCoins((current) => ({
                          ...current,
                          [coin]: e.target.value,
                        }))
                      }
                    />
                  </div>
                ))}
              </fieldset>
            ) : (
              <>
                <Label htmlFor="watch-rule-value">Minimum discount (%)</Label>
                <Input
                  id="watch-rule-value"
                  value={value}
                  inputMode="numeric"
                  required
                  onChange={(e) => setValue(e.target.value)}
                  placeholder="30"
                />
              </>
            )}
            <div className="watchlist-actions">
              <Button type="submit" disabled={pending}>
                Save rule
              </Button>
              <Button type="button" onClick={() => setSelected("")}>
                Cancel
              </Button>
            </div>
          </form>
        )}
        {error && (
          <Alert role="alert">
            {error}{" "}
            <Button onClick={() => setRetry((n) => n + 1)}>
              Reload rules and history
            </Button>
          </Alert>
        )}
        {loading ? (
          <p role="status">Loading rules and history…</p>
        ) : !rules.length ? (
          <p>No alert rules. Select “Add price rule” beside a watched item.</p>
        ) : (
          <div className="table-scroll">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Item</TableHead>
                  <TableHead>Condition</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rules.map((rule) => (
                  <TableRow key={rule.id}>
                    <TableCell>
                      <Button onClick={() => onItem(rule.item_key)}>
                        {names[rule.item_key] ?? rule.item_key}
                      </Button>
                    </TableCell>
                    <TableCell>
                      {ruleName(rule.kind)} ·{" "}
                      {rule.kind === "threshold" ? (
                        <WowMoney
                          value={{
                            numerator: rule.threshold_copper ?? 0,
                            denominator: 1,
                          }}
                        />
                      ) : (
                        `${rule.discount_percent}%`
                      )}
                    </TableCell>
                    <TableCell>{rule.enabled ? "Enabled" : "Paused"}</TableCell>
                    <TableCell>
                      <div className="watchlist-actions">
                        <Button
                          disabled={pending}
                          onClick={() =>
                            void changeRule(
                              `/alert-rules/${rule.id}`,
                              "PATCH",
                              { enabled: !rule.enabled },
                            )
                          }
                        >
                          {rule.enabled ? "Pause" : "Enable"}
                        </Button>
                        <Button
                          disabled={pending}
                          onClick={() =>
                            void changeRule(`/alert-rules/${rule.id}`, "DELETE")
                          }
                        >
                          Delete rule
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </section>
      <section className="watchlist-section" aria-labelledby="watch-history">
        <h2 id="watch-history">Alert history</h2>
        <p>
          Recent matches for this market. A past match does not confirm that a
          listing is still available.
        </p>
        {!loading && !marketEvents.length ? (
          <p>No alert matches for this market yet.</p>
        ) : (
          <div className="table-scroll">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Item</TableHead>
                  <TableHead>Match</TableHead>
                  <TableHead>Observed price / unit</TableHead>
                  <TableHead>Observed · Paris time</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {marketEvents.map((event) => (
                  <TableRow key={event.id}>
                    <TableCell>
                      <Button onClick={() => onItem(event.item_key)}>
                        {names[event.item_key] ?? event.item_key}
                      </Button>
                    </TableCell>
                    <TableCell>
                      {ruleName(event.evidence.signal?.kind ?? "")}
                    </TableCell>
                    <TableCell>
                      <WowMoney value={event.evidence.current_price} />
                    </TableCell>
                    <TableCell>{time(event.observed_at)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </section>
    </section>
  );
}

function WatchedItem({
  itemKey,
  market,
  refresh,
  onItem,
  onRule,
  onName,
}: {
  itemKey: string;
  market: string;
  refresh: number;
  onItem: (key: string) => void;
  onRule: () => void;
  onName: (key: string, name: string) => void;
}) {
  const [item, setItem] = useState<Item | null>(null),
    [error, setError] = useState(""),
    [retry, setRetry] = useState(0),
    [loading, setLoading] = useState(true);
  useEffect(() => {
    let active = true;
    if (!market) {
      setLoading(false);
      return;
    }
    api<Item>(
      `/items/${encodeURIComponent(itemKey)}?${new URLSearchParams({ market_id: market })}`,
    )
      .then((row) => {
        if (active) {
          setItem(row);
          onName(itemKey, row.name);
          setError("");
        }
      })
      .catch((e) => {
        if (active) {
          setItem(null);
          setError(
            e instanceof ApiError && e.status === 404
              ? "No observations in this market"
              : "Could not load price",
          );
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [itemKey, market, refresh, retry, onName]);
  return (
    <TableRow>
      <TableCell>
        <Button className="watchlist-item" onClick={() => onItem(itemKey)}>
          <ItemIcon name={item?.name ?? itemKey} icon={item?.icon} />
          {item?.name ?? itemKey}
        </Button>
        {error && (
          <p role="status">
            {error}{" "}
            <Button onClick={() => setRetry((n) => n + 1)}>Retry price</Button>
          </p>
        )}
      </TableCell>
      <TableCell>
        {loading ? "Loading…" : <WowMoney value={item?.min_buyout} />}
      </TableCell>
      <TableCell>
        <DiscountBadge value={item?.discount_percentage} />
      </TableCell>
      <TableCell>
        {item ? (
          <FreshnessIndicator
            seconds={item.freshness_seconds}
            observedAt={item.observed_at}
          />
        ) : (
          "No observation"
        )}
      </TableCell>
      <TableCell>
        <div className="watchlist-actions">
          <Button onClick={onRule}>Add price rule</Button>
          <WatchlistButton itemKey={itemKey} />
        </div>
      </TableCell>
    </TableRow>
  );
}
