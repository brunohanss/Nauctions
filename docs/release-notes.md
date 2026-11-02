# Nauctions 0.5.1

Windows x64 companion and Nauctions AHDB `0.6.0-nauctions.4`, for Normal / Horde.

## Changes

- Recipe dialogs show a simple component list and a separate materials-to-buy table. Known fixed-yield intermediate recipes expand into their raw materials. Batch quantities include recipe yield and surplus.
- Batch auction profit uses a selling price of 1 copper below the latest lowest asking price. No percentage deduction applies to this modal estimate.
- Auction and merchant profit include a color-coded return-on-cost percentage.
- Merchant estimates use the tooltip sell price when the scan has no sell price. Later scans retain known sell prices. The list shows a reason when merchant profit is unavailable.
- Item tooltips, icons, coin amounts, percentage colors, market medians, strategy sorting, and 12-hour and 24-hour forecasts use the shared presentation.
- Local development builds use a separate app identity and settings directory. Release startup and tray behavior remain available.

## Update

Open Settings > App updates. Check for updates, then download and restart. Existing settings and pairing stay in place. Close WoW before addon installation.

The release includes a signed updater package, checksums, and a source revision record. The installer has no Authenticode publisher certificate.

## Validation and limits

Automated checks cover the crafting purchase chain, profit calculations, tooltip sell-price fallback, later uploads with empty prices, and the database API flow. The release workflow checks the signed package and installer upgrade on a disposable Windows runner before publication.

Prices are observations. A purchase plan needs the intermediate recipe in the shared catalogue. Missing, ambiguous, variable-yield, and cyclic recipes remain direct purchases. Tooltips use Classic reference data when an observed sell price is unavailable. Actual supply and sale prices can change. Live game compatibility and sustained multiplayer use remain separate operational checks.
