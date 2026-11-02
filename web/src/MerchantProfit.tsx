import { WowMoney as CoinMoney } from './WowMoney';
import { useEffect, useState } from "react";
import { api, type Price } from "./api";
import { Alert } from "./components/ui/alert";
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "./components/controls";
export type MerchantQuote = {
  status: string;
  unit_price: Price | null;
  output_units: number | null;
  purchase_cost: Price | null;
  sale_value: Price | null;
  profit: Price | null;
};
export const merchantReasons: Record<string, string> = {
  missing_merchant_price: "Merchant price unavailable",
  not_sellable: "Merchants do not buy this output",
  unknown_or_variable_yield: "Output quantity is unknown or variable",
  missing_materials: "Reagent prices or supply are incomplete",
};
export function MerchantSummary({ quote }: { quote?: MerchantQuote | null }) {
  return (
    <section className="merchant-summary" aria-label="Merchant sale">
      <h3>Craft and sell to merchant</h3>
      {quote?.status === "estimated" ? null : (
        <p className="chart-note">
          {merchantReasons[quote?.status ?? ""] ?? "Merchant estimate unavailable"}
        </p>
      )}
      <dl className="crafting-summary">
        <div>
          <dt>Reagent purchase cost</dt>
          <dd><CoinMoney value={quote?.purchase_cost} /></dd>
        </div>
        <div>
          <dt>Merchant payout</dt>
          <dd><CoinMoney value={quote?.sale_value} /></dd>
        </div>
        <div>
          <dt>Merchant profit</dt>
          <dd><CoinMoney value={quote?.profit} /></dd>
        </div>
      </dl>
      <p className="chart-note">
        Whole reagent stacks, including surplus units. Additional batch costs
        apply. No auction cut.
      </p>
    </section>
  );
}
export function MerchantProductDetails({
  itemKey,
  market,
  unitPrice,
}: {
  itemKey: string;
  market: string;
  unitPrice?: Price | null;
}) {
  const [rows, setRows] = useState<
      {
        recipe_id: string;
        facts: { name: string };
        merchant?: MerchantQuote | null;
      }[]
    >([]),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(true);
  useEffect(() => {
    let active = true;
    setBusy(true);
    setError("");
    setRows([]);
    if (!/^i[0-9]+$/.test(itemKey) || !market) {
      setBusy(false);
      return;
    }
    api<{ recipes: typeof rows }>(
      `/recipes?${new URLSearchParams({ market_id: market, output_item_id: itemKey.slice(1), sort: "merchant_profit", direction: "desc", limit: "5" })}`,
    )
      .then((r) => {
        if (active) setRows(r.recipes);
      })
      .catch((e) => {
        if (active) setError(String(e));
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, [itemKey, market]);
  return (
    <section
      aria-label="Craft and sell to merchant"
      className="merchant-summary"
    >
      <h2>Merchant sale</h2>
      <p>
        Merchant payout per item: <strong><CoinMoney value={unitPrice} /></strong>
      </p>
      <p className="chart-note">
        Crafting results below use one craft and whole-stack reagent purchases.
      </p>
      {busy ? (
        <p role="status">Loading merchant estimates...</p>
      ) : error ? (
        <Alert>{error}</Alert>
      ) : !rows.length ? (
        <p className="chart-note">
          No discovered recipe for this exact output.
        </p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Recipe</TableHead>
              <TableHead className="numeric">Purchase cost</TableHead>
              <TableHead className="numeric">Merchant payout</TableHead>
              <TableHead className="numeric">Merchant profit</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((r) => (
              <TableRow key={r.recipe_id}>
                <TableCell>
                  {r.facts.name}
                  {r.merchant?.status !== "estimated" && (
                    <small>
                      {merchantReasons[r.merchant?.status ?? ""] ?? "Unavailable"}
                    </small>
                  )}
                </TableCell>
                <TableCell className="numeric">
                  <CoinMoney value={r.merchant?.purchase_cost} />
                </TableCell>
                <TableCell className="numeric">
                  <CoinMoney value={r.merchant?.sale_value} />
                </TableCell>
                <TableCell className="numeric">
                  <CoinMoney value={r.merchant?.profit} />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </section>
  );
}
