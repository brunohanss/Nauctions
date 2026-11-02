# Nauctions 0.5.0

Windows x64 companion and Nauctions AHDB `0.6.0-nauctions.4`, for Normal / Horde.

## Changes

- Consistent shadcn UI with dark and light themes across Market, Crafting, Strategy, setup, and Settings.
- One Market page with server sorting, filters, pagination, and item dialogs that preserve your place.
- Crafting compares auction profit with merchant profit. Sort by either profit, cost, output, recipe, profession, or status. Filter for positive merchant profit.
- Recipe dialogs show merchant payout, whole-stack reagent purchase cost, and net merchant profit. Product details show the merchant unit payout and discovered recipes for that exact output.
- Merchant calculations use collected sell prices and available whole reagent stacks. No auction fee applies to merchant sales. Missing prices, insufficient supply, service outputs, variable yields, and known merchant refusals do not produce a profit estimate.
- The addon records recipe-output metadata even when the output has no auction listing. Open a profession window and save with `/reload` or logout to contribute these prices.
- Existing price charts, forecasts, historical rise rates, export, diagnostics, and local recovery remain available. Shared-history writes remain disabled.

## Update

Open Settings > App updates. Check for updates, then download and restart. Close WoW before addon installation. The addon ZIP is also available for manual installation. Existing addon files are backed up; saved game data stays in place.

The release includes a signed updater package, checksums, and a source revision record. The updater signature verifies integrity and version. The installer has no Authenticode publisher certificate.

## Limits

Prices are collected observations. Reagent asks can change before purchase. Merchant prices remain unavailable until collected. Whole-stack costs include surplus units. Auction sales are not guaranteed. Historical rise rates are observations, not calibrated future probabilities.

Automated checks use synthetic fixtures. Live game compatibility, reboot behavior, sustained multi-player use, and an interactive updater handoff remain separate operational checks.
