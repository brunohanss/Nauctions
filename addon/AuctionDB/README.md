# Nauctions AHDB

Install one addon to collect auction scans and shared profession recipe facts.
This fork keeps the `AuctionDB` folder and existing auction SavedVariables. It
replaces upstream AHDB. The desktop collector continues to read `AuctionDB.lua`.

## Install

1. Close WoW.
2. Back up the installed `Interface/AddOns/AuctionDB` folder and the account's
   `SavedVariables/AuctionDB.lua` file.
3. Replace the addon folder with this `AuctionDB` folder. Keep the saved file.
4. Enable Nauctions AHDB in the addon list.
5. Enable **Record full scans** in AHDB settings. Existing settings are retained.
6. Open the auction house and run a scan.
7. Browse recipe items at the auction house or open profession windows. Run
   `/ahdb professions` to inspect capture status.
8. Run `/reload` or log out to save. The Nauctions collector uploads the file.

Use `/ahdb professions scan` to repeat profession capture and retry pending auction
recipe items. Auction browse updates, item searches, and completed full scans all
supply discovery candidates. Full-scan recipe discovery works even when full-scan
history recording is disabled. The addon reads exposed profession recipe
IDs in batches of ten. It uses the unfiltered recipe API when available; otherwise
it reads the filtered list. It does not change window filters. Open more windows
to expand the shared catalogue. Coverage is not a complete list of game recipes.

Auction candidates are selected by numeric item class, not their localized names.
The addon reads the structured tooltip's teaching-spell ID, then validates the
recipe identity and schematic. It uses the item's use spell only as a fallback,
with the same validation. It never assumes that a use spell or finished-item ID
is the crafting spell. Ambiguous teaching spells remain unresolved.

Missing item or recipe data gets at most three attempts per observation pass.
Pending recipe items survive reload and retry at addon load, a later auction
observation, profession opening, or the manual rescan command. Known recipes are
retained if a later API read fails. Work is split into batches of at most 100
item classifications and ten recipe reads per timer step. Status reports the
20,000 recipe-item / recipe limit or 50,000 distinct auction-item session limit.

Update the server before installing version `0.5.0-nauctions.2`. It adds the
optional `recipe_items` export section and migration 0006. Older server parsers
will reject this section. Existing profession-only exports remain supported.

Recipe exports contain no character names, GUIDs, profession ranks, inventory
counts, or learned-recipe flags. Existing upstream auction features and their
saved data remain present. Recipe facts are shared through the group's database.

The module reads the Forever `C_TradeSkillUI` recipe schematic APIs. Alternative
reagents, currency inputs, quality recipes, gathering, salvage and dummy recipes
are reported as unresolved or unsupported. An incomplete capture does not remove
an existing recipe. The addon does not craft items or buy auctions. It needs no
HTTP connection or API key.

Unknown output quantities remain unknown. Enchants with no item output remain
services. Tool and station names are facts; current availability and player
cooldown state are not exported. A new client build or locale starts a new local
observation set. The server retains accepted earlier observations.

## In-game crafting profit

1. Run a full auction scan. Prices must come from one scan in one auction house.
2. Open your own profession window.
3. Click **Crafting profit** above that window, or run `/ahdb crafting`.
4. Select Alliance, Horde, or Neutral prices.
5. Review recipes in descending estimated profit per craft. Use Previous and Next
   to move through the list. Hover a row for details. Click a row for the output
   item's price history.

**Known (buy materials)** lists supported recipes that this character knows in
the open profession. **Ready now** keeps recipes for which the client confirms
available materials, crafting permission, requirements and no active cooldown.
If a readiness check is unavailable, the recipe is not marked ready. Change the
open profession to check another profession. The list refreshes when materials,
recipe data, cooldowns or auction scans change while the profit window is open.
Use Refresh to request a new calculation. Closing the profession clears the list.

Hover a learned or unlearned recipe in the native profession list to see an
additional cost tooltip. Select a recipe, then hover its output icon in the
crafting panel to see the same estimate below the native item details. The
estimate is per craft. It uses the selected recipe, even if no recipe item was
discovered at the auction house. Enchants with no item output show material cost.
Hover a discovered recipe item in the auction house, bags or chat to see the same
estimate in its item tooltip. A recipe item must first have a resolved discovery
mapping. Pending mappings show a message instead of an invented estimate. The
**Prices in item tooltips** option also controls crafting tooltips.

The tooltip shows material cost per craft, each material's required quantity and
cost, the output's unit asking price, total asking value per craft, sale value
after the auction cut, estimated profit, and return on material cost. All materials
are valued at AH prices, including materials already owned. Fixed output quantity
is included in the calculation. The fee assumption is 5% for faction houses and
15% for Neutral; it is not a verified Forever fee contract.

Only prices from the latest saved scan of the selected house are used. Older item
sightings and daily summaries do not fill missing prices.
Scans older than one hour show **STALE**. Scan coverage is not verified. Material
quantity is an observed aggregate; it does not prove that enough buyout stacks
remain. Costs use AHDB's recorded minimum unit prices, which are rounded to whole
copper. They exclude whole-stack surplus, deposits, failed listings, vendor costs,
recursive intermediate crafts and the purchase price of a recipe item. Asking
prices do not prove a sale.

Missing prices, insufficient observed material quantity, variable or unknown yield,
services, and known bound outputs do not produce profit figures. Unknown binding
shows a warning. Character learned flags, materials and readiness stay in memory;
they are not added to shared recipe exports.

The simulated-client tests cover calculations, ranking, readiness, tooltip hooks
and window controls. Validate the layout, hover behavior, save/reload, cooldowns
and profession switching in the real Forever client before release.

## Source and validation

`UPSTREAM.json` identifies the pinned AHDB revision and modified files. The
upstream `LICENSE`, source headers and TOC licence notice are retained. Built-in
game icons replace upstream binary icon files in this package.

The recipe adapter was checked against the Forever UI source documentation:

- https://github.com/Gethe/wow-ui-source/blob/forever/Interface/AddOns/Blizzard_APIDocumentationGenerated/TradeSkillUIDocumentation.lua
- https://github.com/Gethe/wow-ui-source/blob/forever/Interface/AddOns/Blizzard_APIDocumentationGenerated/TradeSkillUITypesDocumentation.lua
- https://github.com/Gethe/wow-ui-source/blob/forever/Interface/AddOns/Blizzard_APIDocumentationGenerated/AuctionHouseDocumentation.lua
- https://github.com/Gethe/wow-ui-source/blob/forever/Interface/AddOns/Blizzard_APIDocumentationGenerated/ItemDocumentation.lua
- https://github.com/Gethe/wow-ui-source/blob/forever/Interface/AddOns/Blizzard_APIDocumentationGenerated/TooltipInfoDocumentation.lua
- https://github.com/Gethe/wow-ui-source/blob/forever/Interface/AddOns/Blizzard_APIDocumentationGenerated/TooltipInfoSharedDocumentation.lua

Simulated-client tests do not prove running-game compatibility. The real-client
release gate remains open. Test save/reload, auction scans, recipe yields and
interrupted captures before a stable release.


## Price forecasts

The app's **Forecasts** page uses the selected market's scan history. A 24-hour
estimate needs a fresh complete scan and three observed days, each with
two scans at least four hours apart. Early trends are provisional and use recent next-day
price changes. Their accuracy is measured as more observations arrive. An
unchanged-price baseline is shown when other methods do not improve sufficiently.
Failed later validation, insufficient history, or stale evidence produces no
estimate.

Use **Download this item for addon** or **Download this page for addon** in the
app to export up to 30 available item forecasts. Close WoW. Extract the ZIP into
`Interface/AddOns`, so `NauctionsForecastCache` is beside `AuctionDB`. Enable
Nauctions Forecast Cache and start WoW. The cache is a separate data addon;
it never replaces `AuctionDB.lua` SavedVariables. Each export replaces the
previous cache. Exact item variants, realm, faction, generation time, and expiry
must match. Expired and wrong-market estimates are hidden. An export expires
within 45 minutes, and never later than 45 minutes after its live observation.

The range uses historical errors. Provisional trends use a minimum log margin
of 0.10; basic and later-validated methods use 0.05. A range is not shown before
error comparisons are available.
It is not a calibrated probability interval. Asking prices do not prove that an
item sold. Running-game compatibility still needs the real-client release trial.
