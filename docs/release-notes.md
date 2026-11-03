# Nauctions 0.6.0

Windows x64 companion and Nauctions AHDB `0.6.0-nauctions.4`, for Normal / Horde.

## Changes

- Add a personal Watchlist tab. Save or remove items from item details.
- Add a Watch list filter after Equipment and Other in Market. Search, sorting, and pagination still apply. The filter also supports older servers.
- Create price-target and discount rules, pause or enable them, and read alert history for the selected market.
- Enter price targets with separate whole-number Gold, Silver, and Copper fields.
- Use Test alert to request one notification. The manual test works while WoW is closed and reports permission or delivery errors.
- Set the gold logo explicitly on the native app window. The taskbar, executable, and tray use the same bundled artwork.

## Update

Open Settings > App updates. Check for updates, then download and restart. Saved settings and pairing remain in place. Close WoW before addon installation.

The release includes a signed updater package, checksums, and a source revision record. The installer has no Authenticode publisher certificate.

## Validation and limits

The release workflow checks the source, signed package, fresh installation, and upgrade from 0.5.1 on disposable runners before publication.

Automatic Windows opportunity notifications and the local WoW-running notification preference are not included. New rules record in-app alert matches. A manual test request does not prove that Windows displayed a toast; Windows notification settings still apply.

Prices are observations. Live game compatibility and sustained multiplayer use remain separate operational checks. This release keeps the existing addon and collector behavior from 0.5.1.
