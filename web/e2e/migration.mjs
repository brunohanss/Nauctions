import AxeBuilder from "@axe-core/playwright";
import { chromium } from "@playwright/test";
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  viewport: { width: 1280, height: 800 },
});
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const now = new Date().toISOString();
const price = (n) => ({ numerator: String(n), denominator: "1" });
const rows = Array.from({ length: 20 }, (_, i) => ({
  item_key: "i" + (10620 + i),
  name: i === 0 ? "Thorium Ore" : "Observed material " + (i + 1),
  category: "materials",
  market_id: "normal",
  observed_at: now,
  received_at: now,
  source_kind: "scan",
  quality: "complete",
  sample_count: 4,
  min_buyout: price(23000 + i * 1100),
  median_buyout: price(25000 + i * 1100),
  total_units: 120,
  listing_count: 20,
  baseline: { price: price(32000), day_count: 7, source_count: 3 },
  freshness_seconds: 30,
  reference_profit: price(7400),
  discount_percentage: -28,
  trend: [],
  icon: "/item-icons/10620.jpg",
}));
const recipe = {
  recipe_id: "recipe-1",
  build: "test",
  facts: {
    spell_id: 1,
    profession_id: 1,
    profession_name: "Alchemy",
    name: "Major Healing Potion",
    kind: "item",
    output_item_id: 13446,
    min_output: 1,
    max_output: 1,
    reagents: { 10620: 2 },
    tools: [],
  },
  last_observed_at: now,
  status: "estimated",
  quote: { total_cost: price(12000), estimated_profit: price(7000) },
  output_icon: "/item-icons/13446.jpg",
};
await page.route("**/v1/**", async (route) => {
  const u = new URL(route.request().url());
  const p = u.pathname.replace("/v1", "");
  let data = {};
  if (p === "/me")
    data = {
      id: "test",
      display_name: "Preview",
      role: "member",
      csrf_token: "test",
    };
  else if (p === "/markets")
    data = [
      {
        id: "normal",
        realm: "Normal",
        house: "Horde",
        last_observed_at: now,
        scan_count: 4,
        source_count: 3,
      },
    ];
  else if (p === "/items")
    data = { items: rows, next_cursor: "page-2", total_count: 80 };
  else if (p === "/forecasts/batch") data = { items: [] };
  else if (p.endsWith("/history"))
    data = {
      points: rows.map((r, i) => ({
        ...r,
        observed_at: new Date(Date.now() - i * 300000).toISOString(),
      })),
      daily_imports: [],
    };
  else if (p.endsWith("/depth"))
    data = { available: true, observed_at: now, listings: [] };
  else if (p.endsWith("/forecast"))
    return route.fulfill({
      status: 503,
      json: { message: "Forecast unavailable in this preview" },
    });
  else if (p.startsWith("/items/")) data = rows[0];
  else if (p === "/professions")
    data = [{ id: 1, name: "Alchemy", recipe_count: 1 }];
  else if (p === "/recipe-items")
    data = { total_count: 1, unresolved_count: 0 };
  else if (p === "/recipes") data = { recipes: [recipe], total_count: 1 };
  else if (p.includes("/quote"))
    data = { recipe, status: "missing_materials", quote: null };
  else if (["/devices", "/sources", "/admin/members", "/watchlists", "/alert-rules", "/alerts"].includes(p)) data = [];
  return route.fulfill({ json: data });
});
await mkdir("../artifacts/ui", { recursive: true });
for (const theme of ["dark", "light"]) {
  await page.goto(`${process.env.UI_ORIGIN || "http://127.0.0.1:1421"}/market`);
  await page.evaluate((t) => {
    localStorage.setItem("nauctions.theme", t);
  }, theme);
  await page.reload();
  await page.getByRole("link", { name: "Thorium Ore", exact: true }).waitFor();
  await page.getByRole("link", { name: "Thorium Ore", exact: true }).hover();
  const itemTooltip = page.getByRole("tooltip");
  await itemTooltip.waitFor();
  assert.match(await itemTooltip.textContent(), /Item Level/);
  assert.match(await itemTooltip.textContent(), /Max Stack/);
  await page.waitForTimeout(200);
  await page.screenshot({ path: `../artifacts/ui/item-tooltip-${theme}.png` });
  await page.keyboard.press("Escape");
  await itemTooltip.waitFor({ state: "hidden" });
  await page.mouse.move(0, 0);
  await page.waitForTimeout(2200);
  await page.waitForTimeout(400);
  await page.screenshot({
    path: `../artifacts/ui/market-${theme}.png`,
    fullPage: true,
  });
  const accessibility = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa"])
    .analyze();
  assert.deepEqual(
    accessibility.violations.map((v) => ({
      id: v.id,
      nodes: v.nodes.map((n) => n.html),
    })),
    [],
    "Market accessibility violations",
  );
  await page.getByRole("link", { name: "Thorium Ore", exact: true }).click();
  await page.getByRole("dialog").waitFor();
  await page.waitForTimeout(400);
  await page.screenshot({ path: `../artifacts/ui/item-${theme}.png` });
  await page.keyboard.press("Escape");
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.getByRole("link", { name: "Crafting", exact: true }).click();
  await page.getByText("Major Healing Potion", { exact: true }).waitFor();
  await page.waitForTimeout(400);
  await page.screenshot({
    path: `../artifacts/ui/crafting-${theme}.png`,
    fullPage: true,
  });
  await page.getByText("Major Healing Potion", { exact: true }).click();
  await page.getByRole("dialog").waitFor();
  await page.keyboard.press("Escape");
}
for (const width of [640, 900, 1280, 1600]) {
  await page.setViewportSize({ width, height: 800 });
  await page.goto(`${process.env.UI_ORIGIN || "http://127.0.0.1:1421"}/market`);
  await page.waitForTimeout(2400);
  assert(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
    `Page overflows at ${width}`,
  );
  await page.evaluate(() => scrollTo(0, 500));
  assert(
    await page
      .locator(".app-header")
      .evaluate((el) => el.getBoundingClientRect().top === 0),
    "Header is not sticky",
  );
}
await page.setViewportSize({ width: 1280, height: 800 });
await page.goto(`${process.env.UI_ORIGIN || "http://127.0.0.1:1421"}/settings`);
await page.waitForTimeout(2400);
const appearance = page.getByRole("combobox").first();
await appearance.click();
await page.getByRole("option", { name: "Dark", exact: true }).click();
assert.equal(
  await page.evaluate(() => document.documentElement.dataset.theme),
  "dark",
);
await page.reload();
await page.waitForTimeout(2400);
assert.equal(
  await page.evaluate(() => document.documentElement.dataset.theme),
  "dark",
);
assert.deepEqual(errors, []);
await browser.close();
console.log("Browser checks passed; screenshots saved to artifacts/ui.");
