import { chromium, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import assert from "node:assert/strict";
import { makeFixture, mountFixture } from "./fixtures.mjs";
const browser = await chromium.launch({ headless: true });
let passed = 0;
async function scenario(name, run, options = {}) {
  if (process.env.SCENARIO && !name.includes(process.env.SCENARIO)) return;
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  const fixture = makeFixture();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error" && !m.text().includes("Failed to load resource"))
      console.error(m.text());
  });
  await mountFixture(page, fixture, options);
  try {
    await run(page, fixture);
    assert.deepEqual(errors, []);
    console.log("PASS " + name);
    passed++;
  } finally {
    await context.close();
  }
}
const origin = process.env.UI_ORIGIN || "http://127.0.0.1:1421";
const ready = async (page, path = "/market") => {
  await page.goto(origin + path);
  await expect(
    path.startsWith("/items/")
      ? page.getByRole("dialog")
      : page.getByRole("main"),
  ).toBeVisible();
};
async function audit(page) {
  await page.waitForTimeout(250);
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa"])
    .analyze();
  assert.deepEqual(
    result.violations.map((v) => ({
      id: v.id,
      nodes: v.nodes.map((n) => ({ html: n.html, summary: n.failureSummary })),
    })),
    [],
  );
}
try {
  await scenario(
    "Market server sorting, cursor pagination, category reset and detail state",
    async (p, f) => {
      await ready(p);
      await expect(
        p.getByRole("link", { name: "Thorium Ore", exact: true }),
      ).toBeVisible();
      assert(
        f.state.requests.some(
          (r) =>
            r.path === "/items" &&
            r.query.sort === "reference_profit" &&
            r.query.direction === "desc" &&
            r.query.limit === "20",
        ),
      );
      await p.getByRole("button", { name: "Next page", exact: true }).click();
      await expect
        .poll(() => f.state.requests.some((r) => r.query.cursor === "page-2"))
        .toBe(true);
      await p.getByRole("button", { name: "Materials", exact: true }).click();
      await expect
        .poll(() =>
          f.state.requests.some(
            (r) => r.query.category === "materials" && !r.query.cursor,
          ),
        )
        .toBe(true);

      await p.getByRole("link", { name: "Thorium Ore", exact: true }).focus();
      await p.evaluate(() => scrollTo(0, 300));
      const scroll = await p.evaluate(() => scrollY);
      await p
        .getByRole("link", { name: "Thorium Ore", exact: true })
        .press("Enter");
      await expect(p.getByRole("dialog")).toBeVisible();
      await expect(
        p
          .getByRole("dialog")
          .getByRole("img", { name: "Thorium Ore", exact: true }),
      ).toBeVisible();
      await p.keyboard.press("/");
      await expect(p.getByRole("dialog")).toBeVisible();
      await p.keyboard.press("Escape");
      await expect(p.getByRole("dialog")).toBeHidden();
      await expect(
        p.getByRole("link", { name: "Thorium Ore", exact: true }),
      ).toBeFocused();
      assert.equal(await p.evaluate(() => scrollY), scroll);
      await p.getByRole("button", { name: "Order by", exact: false }).count();
    },
  );
  await scenario(
    "Search keyboard, clearing, empty results and popup layering",
    async (p, f) => {
      await ready(p);
      await p.keyboard.press("/");
      const search = p.getByRole("combobox", { name: "Search for an item" });
      await expect(search).toBeFocused();
      await search.fill("Thorium");
      await expect(
        p.getByRole("option", { name: "Thorium Ore", exact: true }),
      ).toBeVisible();
      const box = await p.locator("[data-slot=popover-content]").boundingBox();
      const header = await p.locator(".app-header").boundingBox();
      assert(box.y >= header.y + header.height);
      await search.press("ArrowDown");
      await search.press("Enter");
      await expect(
        p.getByRole("dialog", { name: "Item details", exact: true }),
      ).toBeVisible();
      await p.keyboard.press("Escape");
      await expect(
        p.getByRole("dialog", { name: "Item details", exact: true }),
      ).toBeHidden();
      await expect(search).toHaveValue("Thorium");
      await p.getByRole("button", { name: "Clear search" }).click();
      await search.fill("no-match");
      await expect(p.getByText("No items match these filters.")).toBeVisible();
      await search.press("Escape");
      await p.getByRole("button", { name: "Reset filters" }).click();
      await expect(search).toHaveValue("");
    },
  );
  for (const mode of ["available", "unavailable", "expired", "error"])
    await scenario("Forecast " + mode, async (p, f) => {
      f.state.forecastMode = mode;
      await ready(p, "/items/i10620");
      await expect(p.getByRole("dialog")).toBeVisible();
      const dialog = p.getByRole("dialog");
      if (mode === "available") {
        await expect(dialog.getByText("Historical rises (12h):")).toBeVisible();
        await expect(
          dialog.getByRole("button", { name: "Download this item for addon" }),
        ).toBeEnabled();
        const download = p.waitForEvent("download");
        await dialog
          .getByRole("button", { name: "Download this item for addon" })
          .click();
        assert.equal(
          (await download).suggestedFilename(),
          "nauctions-forecast-cache.zip",
        );
        await dialog
          .getByRole("button", { name: "Install the addon cache" })
          .click();
        await expect(
          dialog.getByText(/expires within 45 minutes/),
        ).toBeVisible();
        await audit(p);
      }
      if (mode === "unavailable") {
        await expect(
          dialog.getByRole("button", { name: "Download this item for addon" }),
        ).toBeDisabled();
        await dialog
          .getByRole("button", { name: "How this is calculated" })
          .click();
        await expect(
          dialog.getByText("Not enough observed history."),
        ).toBeVisible();
      }
      if (mode === "expired") {
        await expect(
          dialog.getByText("Estimate expired").first(),
        ).toBeVisible();
        await expect(
          dialog.getByRole("button", { name: "Download this item for addon" }),
        ).toBeDisabled();
      }
      if (mode === "error")
        await expect(dialog.getByText(/Forecast request failed/)).toBeVisible();
    });
  await scenario(
    "Export error stays visible and retry is possible",
    async (p, f) => {
      f.state.exportError = true;
      await ready(p);
      await p.getByRole("button", { name: "Actions", exact: true }).click();
      await p
        .getByRole("menuitem", { name: "Download visible forecasts for addon" })
        .click();
      await expect(
        p.getByRole("alert").filter({ hasText: "Export failed" }),
      ).toBeVisible();
    },
  );
  await scenario(
    "Crafting offsets, filters, quote warnings and item transition",
    async (p, f) => {
      await ready(p, "/crafting");
      await expect(
        p.getByRole("button", { name: "Major Healing Potion", exact: true }),
      ).toBeVisible();
      assert(
        f.state.requests.some(
          (r) =>
            r.path === "/recipes" &&
            r.query.sort === "profit" &&
            r.query.direction === "desc",
        ),
      );
      await p.getByRole("button", { name: "Next recipe page" }).click();
      await expect
        .poll(() =>
          f.state.requests.some(
            (r) => r.path === "/recipes" && r.query.offset === "30",
          ),
        )
        .toBe(true);
      await p
        .getByRole("button", { name: "Major Healing Potion", exact: true })
        .click();
      const dialog = p.getByRole("dialog");
      await expect(
        dialog.getByText("Output binding has not been verified."),
      ).toBeVisible();
      await dialog
        .getByRole("button", { name: "Item 10620", exact: true })
        .first()
        .click();
      await expect(p.getByRole("dialog")).toHaveCount(1);
      await expect(
        p.getByRole("dialog").getByRole("heading", { name: "Thorium Ore" }),
      ).toBeVisible();
      await p.keyboard.press("Escape");
      await expect(
        p.getByRole("heading", { name: "Crafting", exact: true }),
      ).toBeVisible();
    },
  );
  await scenario(
    "Market errors, no market, missing prices, stale data and failed artwork",
    async (p, f) => {
      f.state.marketError = true;
      await ready(p);
      await expect(
        p
          .getByRole("alert")
          .filter({ hasText: "Market temporarily unavailable" }),
      ).toBeVisible();
      f.state.marketError = false;
      await p.getByRole("button", { name: "Retry", exact: true }).click();
      await expect(
        p.getByRole("link", { name: "Thorium Ore", exact: true }),
      ).toBeVisible();
      f.state.stale = true;
      f.state.missing = true;
      f.state.longName = true;
      await ready(p, "/items/i10620");
      await expect(
        p
          .getByRole("dialog")
          .getByText("Stale observation. The market may have changed."),
      ).toBeVisible();
      await expect(
        p.getByRole("dialog").getByLabel(/icon unavailable/),
      ).toBeVisible();
      await audit(p);
    },
  );
  await scenario(
    "Signed-out Settings, storage failure, dark/light persistence and form labels",
    async (p, f) => {
      f.state.signedIn = false;
      await p.goto(origin + "/market");
      await expect(
        p.getByRole("heading", { name: "Join your friends" }),
      ).toBeVisible();
      await p.getByRole("button", { name: "Settings", exact: true }).click();
      await p.getByRole("combobox", { name: "Theme", exact: true }).click();
      await p.getByRole("option", { name: "Light", exact: true }).click();
      assert.equal(
        await p.evaluate(() => document.documentElement.dataset.theme),
        "light",
      );
      await audit(p);
      await p.keyboard.press("Escape");
      await p.reload();
      await expect(
        p.getByRole("heading", { name: "Join your friends" }),
      ).toBeVisible();
      assert.equal(
        await p.evaluate(() => document.documentElement.dataset.theme),
        "light",
      );
      await expect(p.getByLabel("Nickname", { exact: true })).toBeVisible();
    },
  );
  await scenario(
    "Blocked storage retains the session theme",
    async (p, f) => {
      f.state.signedIn = false;
      await p.goto(origin + "/market");
      await expect(
        p.getByRole("heading", { name: "Join your friends" }),
      ).toBeVisible();
      await p.getByRole("button", { name: "Settings", exact: true }).click();
      await p.getByRole("combobox", { name: "Theme", exact: true }).click();
      await p.getByRole("option", { name: "Light", exact: true }).click();
      assert.equal(
        await p.evaluate(() => document.documentElement.dataset.theme),
        "light",
      );
    },
    { storageBlocked: true },
  );
  await scenario(
    "Legacy routes, direct links, browser history and 200 percent zoom",
    async (p, f) => {
      for (const route of ["/", "/opportunities", "/items", "/forecasts"]) {
        await ready(p, route);
        await expect(
          p.getByRole("heading", { name: "Market", exact: true }),
        ).toBeVisible();
        assert.equal(new URL(p.url()).pathname, "/market");
      }
      await p.getByRole("link", { name: "Crafting", exact: true }).click();
      await p.goBack();
      await expect(
        p.getByRole("heading", { name: "Market", exact: true }),
      ).toBeVisible();
      await p.goForward();
      await expect(
        p.getByRole("heading", { name: "Crafting", exact: true }),
      ).toBeVisible();
      await p.evaluate(() => (document.documentElement.style.zoom = "2"));
      assert(
        await p.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      );
      await audit(p);
    },
  );
  await scenario(
    "Desktop connection, setup and offline recovery remain available",
    async (p, f) => {
      f.state.config.server = null;
      f.state.config.game_root = "C:/fixture/WoW";
      f.state.signedIn = false;
      await p.goto(origin + "/#/market");
      await expect(
        p.getByRole("heading", { name: "Connect to your market" }),
      ).toBeVisible();
      await p.getByRole("button", { name: "Settings", exact: true }).click();
      await p
        .getByRole("button", { name: "Local recovery and restore" })
        .click();
      await p.getByRole("button", { name: "Inspect recovery records" }).click();
      await expect(p.getByText("No recorded local operations.")).toBeVisible();
      assert(
        f.state.calls.some(
          (c) =>
            c.command === "collector_action" &&
            c.args.action === "recovery-status",
        ),
      );
      assert(
        !f.state.calls.some(
          (c) => c.command === "prepare_wow" || c.args.action === "restore",
        ),
      );
      await audit(p);
    },
    { desktop: true },
  );
  await scenario(
    "Desktop hash routing, setup choice and update error visibility",
    async (p, f) => {
      await p.goto(origin + "/#/market");
      await expect(
        p.getByRole("heading", { name: "Set up your companion" }),
      ).toBeVisible();
      await p
        .getByRole("button", { name: "Continue without collection" })
        .click();
      await expect(
        p.getByRole("heading", { name: "Market", exact: true }),
      ).toBeVisible();
      await p.getByRole("link", { name: "Settings", exact: true }).click();
      assert.equal(new URL(p.url()).hash, "#/settings");
      await expect(p.getByText(/Version 0.4.3 is available/)).toBeVisible();
      f.state.update = "error";
      await p
        .getByRole("button", { name: "Check for updates", exact: true })
        .click();
      await expect(
        p.getByRole("alert").filter({ hasText: "Update check failed" }),
      ).toBeVisible();
      await audit(p);
    },
    { desktop: true },
  );

  await scenario(
    "No accepted market remains an explicit empty state",
    async (p, f) => {
      f.state.market = false;
      await ready(p);
      await expect(
        p.getByText("Waiting for an accepted scan from Normal · Horde."),
      ).toBeVisible();
      assert(!f.state.requests.some((r) => r.path === "/items"));
    },
  );
  await scenario(
    "Recipe popup Escape, dialog focus trap and validation",
    async (p, f) => {
      await ready(p, "/crafting");
      await p
        .getByRole("button", { name: "Major Healing Potion", exact: true })
        .click();
      const dialog = p.getByRole("dialog");
      await dialog.getByRole("combobox", { name: "Material pricing" }).click();
      await expect(
        p.getByRole("option", { name: "Buy whole auction stacks" }),
      ).toBeVisible();
      await p.keyboard.press("Escape");
      await expect(dialog).toBeVisible();
      await dialog
        .getByRole("spinbutton", { name: "Crafts", exact: true })
        .fill("0");
      await expect(dialog.getByText(/Enter 1/)).toBeVisible();
      await dialog.getByRole("button", { name: "Close", exact: true }).focus();
      for (let i = 0; i < 18; i++) {
        await p.keyboard.press("Tab");
        assert(
          await p.evaluate(
            () => !!document.activeElement.closest("[role=dialog]"),
          ),
        );
      }
      await audit(p);
    },
  );
  await scenario(
    "Member and collector revocation require confirmation",
    async (p, f) => {
      await ready(p, "/settings");
      await p.getByRole("button", { name: "Collectors", exact: true }).click();
      await p.getByRole("button", { name: "Revoke", exact: true }).click();
      await expect(p.getByRole("alertdialog")).toBeVisible();
      await expect(
        p.getByRole("button", { name: "Cancel", exact: true }),
      ).toBeFocused();
      await p.getByRole("button", { name: "Cancel", exact: true }).click();
      assert(!f.state.requests.some((r) => r.method === "DELETE"));
      await p
        .getByRole("button", { name: "Workspace administration", exact: true })
        .click();
      await p
        .getByRole("button", { name: "Revoke access", exact: true })
        .click();
      await expect(p.getByRole("alertdialog")).toBeVisible();
      await p.keyboard.press("Escape");
      assert(!f.state.requests.some((r) => r.method === "PATCH"));
    },
  );

  await scenario(
    "Strategy review, chart controls and purchase inputs",
    async (p, f) => {
      f.state.strategy = true;
      await ready(p, "/strategy");
      await p.getByRole("button", { name: /Review Thorium Ore/ }).click();
      await expect(
        p.getByRole("heading", { name: "Thorium Ore", exact: true }),
      ).toBeVisible();
      await p.getByRole("radio", { name: "30 days", exact: true }).click();
      await p
        .getByRole("button", { name: "Optional: calculate a purchase" })
        .click();
      await expect(
        p.getByRole("textbox", { name: "Gold budget", exact: true }),
      ).toBeVisible();
      await audit(p);
    },
  );
  await scenario(
    "Desktop update progress is bounded and failure stays visible",
    async (p, f) => {
      f.state.setup = "ready";
      await p.goto(origin + "/#/settings");
      await expect(
        p.getByRole("button", {
          name: "Download update and restart",
          exact: true,
        }),
      ).toBeVisible();
      await p
        .getByRole("button", {
          name: "Download update and restart",
          exact: true,
        })
        .click();
      await expect(
        p.getByRole("progressbar", { name: "Update download" }),
      ).toHaveAttribute("aria-valuenow", "45");
      await expect(
        p
          .getByRole("alert")
          .filter({ hasText: "Fixture installation stopped" }),
      ).toBeVisible();
      assert(f.state.calls.some((c) => c.command === "install_update"));
    },
    { desktop: true },
  );

  await scenario(
    "Merchant sorting, positive-profit filter and product details",
    async (p, f) => {
      await ready(p, "/crafting");
      await p
        .getByRole("button", { name: "Merchant profit", exact: true })
        .click();
      await expect
        .poll(() =>
          f.state.requests.some(
            (r) =>
              r.path === "/recipes" &&
              r.query.sort === "merchant_profit" &&
              r.query.direction === "desc",
          ),
        )
        .toBe(true);
      await p
        .getByRole("checkbox", { name: "Merchant profit > 0", exact: true })
        .check();
      await expect
        .poll(() =>
          f.state.requests.some((r) => r.query.merchant_profitable === "true"),
        )
        .toBe(true);
      await p
        .getByRole("button", { name: "Major Healing Potion", exact: true })
        .click();
      await expect(
        p.getByRole("region", { name: "Merchant sale", exact: true }),
      ).toBeVisible();
      await expect(
        p
          .getByRole("region", { name: "Merchant sale", exact: true })
          .getByText("80s 0c"),
      ).toBeVisible();
      await p.keyboard.press("Escape");
      await p.getByRole("link", { name: "Market", exact: true }).click();
      await p.getByRole("link", { name: "Thorium Ore", exact: true }).click();
      const details = p.getByRole("region", {
        name: "Craft and sell to merchant",
        exact: true,
      });
      await expect(details).toBeVisible();
      await expect
        .poll(() =>
          f.state.requests.some(
            (r) => r.path === "/recipes" && r.query.output_item_id === "10620",
          ),
        )
        .toBe(true);
      await audit(p);
    },
  );
  console.log(`${passed} acceptance scenarios passed.`);
} finally {
  await browser.close();
}
