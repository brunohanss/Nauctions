export const price = (n) => ({ numerator: String(n), denominator: "1" });
export function makeFixture() {
  const now = Date.now();
  const observed = new Date(now).toISOString();
  const item = {
    item_key: "i10620",
    name: "Thorium Ore",
    category: "materials",
    market_id: "normal",
    observed_at: observed,
    received_at: observed,
    source_kind: "scan",
    quality: "complete",
    sample_count: 4,
    merchant_unit_price: price(20000),
    min_buyout: price(23000),
    median_buyout: price(25000),
    total_units: 120,
    listing_count: 20,
    baseline: { price: price(32000), day_count: 7, source_count: 3 },
    freshness_seconds: 30,
    reference_profit: price(7400),
    discount_percentage: -28,
    trend: [],
    icon: "/item-icons/10620.jpg",
    catalog_attribution: {
      text: "Icons: Blizzard. Mapping: Wowhead Classic.",
      url: "https://www.wowhead.com/classic",
    },
  };
  const estimate = {
    status: "baseline",
    reason: "Unchanged-price baseline from fresh scans.",
    method: "unchanged_price",
    horizon_hours: 24,
    target_at: Math.floor(now / 1000) + 86400,
    price: price(27000),
    low: price(20000),
    high: price(30000),
    change_percent: 8,
    direction: "up",
    observations: 480,
    validation_pairs: 20,
    mean_absolute_log_error: 0.1,
    baseline_error: 0.12,
    range_kind: "historical_error",
  };
  const forecast = {
    schema_version: 1,
    market_id: "normal",
    realm: "CAMWOW EU4 Realm 3 PvE",
    house: "Horde",
    item_key: item.item_key,
    generated_at: Math.floor(now / 1000) - 10,
    expires_at: Math.floor(now / 1000) + 2600,
    demo: false,
    rolling: estimate,
    rolling_12h: { ...estimate, horizon_hours: 12, target_at: Math.floor(now / 1000) + 43200, price: price(26000), change_percent: 4 },
    rise_frequencies: [12, 24].map((h) => ({
      horizon_hours: h,
      status: "available",
      reason: "Comparable observed periods.",
      percent: 60,
      rises: 12,
      comparisons: 20,
      method: "observed",
    })),
    current_median: price(25000),
  };
  const merchant = {
    status: "estimated",
    unit_price: price(20000),
    output_units: 1,
    purchase_cost: price(12000),
    sale_value: price(20000),
    profit: price(8000),
  };
  const recipe = {
    merchant,
    recipe_id: "r1",
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
    last_observed_at: observed,
    status: "estimated",
    quote: { total_cost: price(12000), estimated_profit: price(7000) },
    output_icon: "/item-icons/13446.jpg",
  };
  const state = {
    watched: [],
    alertRules: [],
    alertEvents: [],
    watchError: false,
    signedIn: true,
    market: true,
    empty: false,
    marketError: false,
    forecastMode: "available",
    exportError: false,
    stale: false,
    missing: false,
    longName: false,
    config: {
      server: "https://fixture.invalid",
      game_root: null,
      game_exe: null,
      launch_at_login: false,
      automatic_collection: false,
    },
    setup: "choose",
    update: "available",
    calls: [],
    requests: [],
  };
  function reply(path, method = "GET", body) {
    const url = new URL(path, "http://fixture");
    const p = url.pathname;
    state.requests.push({ path: p, query: Object.fromEntries(url.searchParams), method, body });
    if (p === '/watchlists') {
      if (state.watchError) return { status: 503, body: { message: 'Watchlist unavailable' } };
      if (method === 'POST') { if (!state.watched.includes(body.item_key)) state.watched.push(body.item_key); return { body: { ok: true } }; }
      return { body: state.watched.map(item_key => ({ item_key })) };
    }
    if (p.startsWith('/watchlists/') && method === 'DELETE') {
      if (state.watchError) return { status: 503, body: { message: 'Watchlist unavailable' } };
      state.watched = state.watched.filter(key => key !== decodeURIComponent(p.slice('/watchlists/'.length)));
      return { body: { ok: true } };
    }
    if (p === '/alert-rules') {
      if (method === 'POST') { const rule = { id: `rule-${state.alertRules.length + 1}`, discount_percent: 30, ...body }; state.alertRules.unshift(rule); return { body: rule }; }
      return { body: state.alertRules };
    }
    if (p.startsWith('/alert-rules/')) {
      const id = p.slice('/alert-rules/'.length);
      if (method === 'DELETE') { state.alertRules = state.alertRules.filter(rule => rule.id !== id); return { body: { ok: true } }; }
      const rule = state.alertRules.find(rule => rule.id === id);
      Object.assign(rule, body);
      return { body: rule };
    }
    if (p === '/alerts') return { body: state.alertEvents };
    const current = {
      ...item,
      ...(state.longName
        ? {
            name: "Thorium Ore with an exceptionally long catalogue name ".repeat(
              4,
            ),
            icon: "/missing-artwork.jpg",
          }
        : {}),
      ...(state.stale
        ? {
            freshness_status: "stale",
            observed_at: new Date(now - 7200000).toISOString(),
          }
        : {}),
      ...(state.missing
        ? {
            min_buyout: null,
            median_buyout: null,
            reference_profit: null,
            baseline: { price: null, day_count: 0, source_count: 0 },
          }
        : {}),
    };
    const f = {
      ...forecast,
      ...(state.forecastMode === "expired"
        ? {
            generated_at: Math.floor(now / 1000) - 4000,
            expires_at: Math.floor(now / 1000) - 1400,
          }
        : {}),
      rolling: {
        ...estimate,
        ...(state.forecastMode === "unavailable"
          ? {
              status: "insufficient",
              price: null,
              low: null,
              high: null,
              reason: "Not enough observed history.",
            }
          : {}),
      },
    };
    if (p === "/me")
      return state.signedIn
        ? {
            body: {
              id: "test",
              display_name: "Preview",
              role: "admin",
              csrf_token: "fixture",
            },
          }
        : { status: 401, body: { message: "Sign in required" } };
    if (p === "/markets")
      return {
        body: state.market
          ? [
              {
                id: "normal",
                realm: "CAMWOW EU4 Realm 3 PvE",
                house: "Horde",
                last_observed_at: state.noScanYet ? null : observed,
                scan_count: 4,
                source_count: 3,
              },
            ]
          : [],
      };
    if (p === "/items") {
      if (url.searchParams.get('category') === 'watchlist') {
        if (state.legacyWatchlist) return { status: 400, body: { error: 'invalid_category' } };
        const items = state.watched.includes(current.item_key) && !state.empty && (url.searchParams.get('q') ?? '') !== 'no-match' ? [current] : [];
        return { body: { items, next_cursor: null, total_count: items.length } };
      }
      if (state.legacyWatchlist && url.searchParams.get('limit') === '100') {
        const last = url.searchParams.get('cursor') === '100';
        return { body: { items: last ? [current] : [{ ...current, item_key: 'unwatched', name: 'Unwatched material' }], next_cursor: last ? null : '100', total_count: 2 } };
      }
      if (state.marketError)
        return {
          status: 503,
          body: { message: "Market temporarily unavailable" },
        };
      return {
        body: {
          items:
            state.empty || url.searchParams.get("q") === "no-match"
              ? []
              : Array.from(
                  { length: url.searchParams.get("limit") === "12" ? 1 : 20 },
                  (_, i) => ({
                    ...current,
                    item_key: "i" + (10620 + i),
                    name: i ? "Observed material " + i : current.name,
                  }),
                ),
          next_cursor: "page-2",
          total_count: 80,
        },
      };
    }
    if (p === "/forecasts/batch") return { body: { items: [f] } };
    if (p === "/forecasts/addon")
      return state.exportError
        ? { status: 503, body: { message: "Export failed. Try again." } }
        : {
            body: {
              files: [
                {
                  name: "NauctionsForecastCache/NauctionsForecastCache.toc",
                  content: "## Interface: 11500",
                },
                {
                  name: "NauctionsForecastCache/ForecastData.lua",
                  content: "NauctionsForecasts = {}",
                },
              ],
            },
          };
    if (p.endsWith("/forecast"))
      return state.forecastMode === "error"
        ? { status: 503, body: { message: "Forecast request failed" } }
        : { body: f };
    if (p.endsWith("/history"))
      return {
        body: {
          points: Array.from({ length: 20 }, (_, i) => ({
            ...item,
            observed_at: new Date(now - (20 - i) * 300000).toISOString(),
          })),
          daily_imports: [],
        },
      };
    if (p.endsWith("/depth"))
      return { body: { available: true, observed_at: observed, listings: [] } };
    if (p.startsWith("/items/")) return { body: current };
    if (p === "/professions")
      return { body: [{ id: 1, name: "Alchemy", recipe_count: 60 }] };
    if (p === "/recipe-items")
      return { body: { total_count: 60, unresolved_count: 2 } };
    if (p.startsWith("/item-metadata/")) return { body: { name: p.endsWith("14048") ? "Bolt of Runecloth" : "Rune Thread", quality: 1, tooltip: "" } };
    if (p === "/recipes" && url.searchParams.has("output_item_id") && !url.searchParams.has("sort") && !state.craftingTree) return { body: { recipes: [recipe], total_count: 1 } };
    if (p === "/recipes" && url.searchParams.has("output_item_id") && url.searchParams.get("sort") === "name") {
      const intermediate = state.craftingTree && url.searchParams.get("output_item_id") === "14048" ? {
        ...recipe, recipe_id: "bolt", facts: { ...recipe.facts, spell_id: 2, name: "Bolt of Runecloth", output_item_id: 14048, min_output: 1, max_output: 1, reagents: { 14047: 5 } },
      } : null;
      return { body: { recipes: intermediate ? [intermediate] : [], total_count: intermediate ? 1 : 0 } };
    }
    if (p.includes("/quote") && state.craftingTree) {
      const intermediate = p.includes("/2/");
      const facts = intermediate ? { ...recipe.facts, spell_id: 2, name: "Bolt of Runecloth", output_item_id: 14048, reagents: { 14047: 5 } } : recipe.facts;
      const crafts = Number(url.searchParams.get("crafts") ?? 1);
      const reagents = Object.entries(facts.reagents).map(([id, units]) => ({ item_id: Number(id), required_units: units * crafts, unit_price: price(id === "14048" ? 100 : 10), cost: price(units * crafts * (id === "14048" ? 100 : 10)), status: "priced" }));
      const cost = reagents.reduce((sum, line) => sum + Number(line.cost.numerator), 0);
      return { body: { recipe: { ...recipe, facts }, status: "estimated", snapshot_id: "craft-tree", quote: {
        reagents, material_cost: price(cost), output_units: crafts, sale_unit_price: price(1000), gross_sale_value: price(1000 * crafts), estimated_profit: price(1000 * crafts - cost),
      } } };
    }
    if (p === "/recipes")
      return {
        body: {
          recipes: state.empty
            ? []
            : Array.from({ length: 30 }, (_, i) => ({
                ...recipe,
                recipe_id: "r" + i,
                facts: {
                  ...recipe.facts,
                  name: i ? "Recipe " + i : recipe.facts.name,
                },
              })),
          total_count: 60,
        },
      };
    if (p.includes("/quote"))
      return {
        body: {
          recipe,
          merchant,
          status: "missing_materials",
          stale: true,
          binding_unknown: true,
          quote: state.pricedIngredients ? {
            material_cost: price(120 * Number(url.searchParams.get("crafts") ?? 1)),
            output_units: Number(url.searchParams.get("crafts") ?? 1),
            sale_unit_price: price(200),
            gross_sale_value: price(200 * Number(url.searchParams.get("crafts") ?? 1)),
            net_sale_value: price(200 * Number(url.searchParams.get("crafts") ?? 1) * (1 - Number(url.searchParams.get("fee_bps") ?? 0) / 10000)),
            estimated_profit: price((200 * (1 - Number(url.searchParams.get("fee_bps") ?? 0) / 10000) - 120) * Number(url.searchParams.get("crafts") ?? 1)),
            reagents: [{ item_id: 10620,
              required_units: 2 * Number(url.searchParams.get("crafts") ?? 1),
              unit_price: price(60), cost: price(120 * Number(url.searchParams.get("crafts") ?? 1)), status: "priced" }],
          } : null,
        },
      };
    if (p === "/strategies")
      return {
        body: {
          items: state.strategy
            ? [
                {
                  item,
                  heatmap: [],
                  patterns: [
                    {
                      detected: true,
                      strategy: "weekly_cycle",
                      status: "watch",
                      action: "Review entry",
                      reason: "Observed weekly price pattern.",
                      entry_reference: price(25000),
                      target: price(32000),
                      conservative_target: price(28000),
                      holding_hours: 18,
                      holding_range_hours: [12, 24],
                      historical_pairs: [],
                      pairs: 20,
                      training_pairs: 20,
                      validation_pairs: 20,
                      validation_positive: 12,
                      coverage_days: 30,
                      observed_weeks: 4,
                    },
                  ],
                },
              ]
            : [],
          as_of: observed,
        },
      };
    if (p === "/devices")
      return {
        body: [
          {
            id: "collector",
            display_name: "Preview collector",
            status: "Paired",
            last_seen_at: observed,
            health: { queue_count: 0 },
          },
        ],
      };
    if (p === "/admin/members")
      return {
        body: [
          {
            id: "other",
            display_name: "Other member",
            role: "member",
            status: "active",
          },
        ],
      };
    if (p === "/admin/sources") return { body: [] };
    if (p === "/auth/logout") {
      state.signedIn = false;
      return { body: {} };
    }
    return { body: {} };
  }
  async function invoke(command, args = {}) {
    state.calls.push({ command, args });
    if (command === 'send_test_alert') {
      if (state.testAlertError) throw new Error('Test notification unavailable');
      return null;
    }
    if (command === "observability_preference") return false;
    if (command === "desktop_config") return state.config;
    if (command === "api_request") {
      const r = reply(args.path, args.method, args.body);
      return { status: r.status || 200, body: r.body };
    }
    if (command === "discover_wow")
      return state.setup === "ready" ? ["C:/fixture/WoW"] : [];
    if (command === "prepare_wow" || command === "auto_connect_collector")
      return null;
    if (command === "collector_state")
      return {
        running: false,
        paired: false,
        game_state: "closed",
        capture_mode: null,
        startup_enabled: false,
        startup_error: null,
      };
    if (command === "check_update")
      return state.update === "error"
        ? Promise.reject("Update check failed")
        : {
            current_version: "0.4.2",
            version: state.update === "available" ? "0.4.3" : null,
            notes: "Fixture release notes",
          };
    if (command === "diagnostics_snapshot")
      return { fixture: true, server: "offline" };
    if (command === "collector_action")
      return JSON.stringify({ operations: [] });
    return null;
  }
  return { state, item, forecast, recipe, reply, invoke };
}
export async function mountFixture(
  page,
  fixture,
  { desktop = false, storageBlocked = false } = {},
) {
  if (storageBlocked)
    await page.addInitScript(() => {
      Storage.prototype.getItem = () => {
        throw new Error("Storage unavailable");
      };
      Storage.prototype.setItem = () => {
        throw new Error("Storage unavailable");
      };
    });
  if (desktop) {
    await page.exposeFunction("__fixtureInvoke", (cmd, args) =>
      fixture.invoke(cmd, args),
    );
    await page.addInitScript(() => {
      window.isTauri = true;
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd, args) => {
          if (cmd === "install_update") {
            await window.__fixtureInvoke(cmd, {});
            args.progress.onmessage({
              phase: "downloading",
              downloaded: 45,
              total: 100,
            });
            await new Promise((resolve) => setTimeout(resolve, 1800));
            throw new Error("Fixture installation stopped");
          }
          return window.__fixtureInvoke(cmd, args);
        },
        transformCallback: () => 1,
        unregisterCallback: () => {},
      };
    });
  }
  await page.route("**/v1/**", async (route) => {
    const r = fixture.reply(
      new URL(route.request().url()).pathname.slice(3) +
        new URL(route.request().url()).search,
      route.request().method(),
      route.request().postDataJSON(),
    );
    await route.fulfill({ status: r.status || 200, json: r.body });
  });
}
