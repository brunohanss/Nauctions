# External item catalogue and market data

## Current status

AHledger documents Forever item metadata with icons, per-market price history, and a price table. These are AHledger records, not AHDB SavedVariables exports.

- Source: https://ahledger.com/data
- API contract and attribution: https://ahledger.com/developers
- Item lookup: https://api.ahledger.com/v1/items/2589?game=forever
- Market lookup: https://api.ahledger.com/v1/markets

On 27 October 2026, direct API requests failed. The user reported that Bitdefender blocked them. No security exception was added. No further AHledger requests were made after that report. No AHledger price records were imported. Documentation examples contain future dates and must not be used as actual observations.

## Local icons

`assets/item-catalog.json` now covers all 50 Classic item IDs in the simulated market. Each mapping was verified against a matching Wowhead Classic item name and icon field. The JPEG files come from Blizzard's image CDN and are served from `web/public/item-icons`. See `assets/ITEM-ICONS.md` for source evidence, file hashes, and the repeatable downloader. These are Classic display assets; Forever client compatibility still requires live-client validation.

The server returns `icon` and `catalog_attribution` for known IDs. Lists, search, and item details show source credit when catalogue icons are used. Unknown IDs keep a neutral placeholder. The catalogue does not replace observed prices, item names, or categories. The synthetic IDs used in the initial test data are not real item mappings.

To extend the catalogue, add verified item IDs, local image paths, source links, and original image URLs. Download each image to the matching local path. Do not infer an icon from an unverified Retail mapping. Build both the web app and server after a catalogue update. Production Docker builds include the catalogue.

## Market data integration requirements

When source access is available:

1. Read actual API responses. Do not use documentation examples as fixtures labelled real.
2. Preserve game, market ID, region, faction, source URL, observation time, and aggregation interval.
3. Keep AHledger records separate from contributor scans. Do not upload them as AHDB scans or feed them through collector history export.
4. Label hourly and daily aggregates. Do not present aggregate points as full scan listings.
5. Show AHledger credit and retain source links. Do not resell or re-export its data as a feed.
6. Respect response cache headers and rate limits.
7. Keep original dates. Any replay view must state its evaluation time.

The documented 24-hour granularity is inconsistent: one section says full scans and another says hourly buckets. Verify actual response semantics before importing history. Verify the source's market-to-realm mapping; its ruleset market IDs must not be silently assigned to a local realm or to EU.
