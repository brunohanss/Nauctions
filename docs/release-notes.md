# Nauctions 0.4.2

Windows x64 companion update, with Nauctions AHDB `0.6.0-nauctions.3`, for Normal / Horde.

## What changed

- Open item details in a modal without losing your place in the list. Details show the item picture, price history, and the available 24-hour forecast.
- Compare the current price with the seven-day median. See historical price-rise rates for 12 and 24 hours when enough recent evidence is available.
- Sort opportunities by estimated margin. Crafting starts with the highest estimated profit and uses the same compact detail flow.
- Automatic setup finds WoW, installs the bundled addon, and connects collection. Existing addon files are backed up under `NauctionsBackups`; saved game data stays in place.
- Clearer startup, settings access, and tray collection status. Retired beta reference prices are removed.

## Update

In your companion, open **Settings > App updates**, check for updates, then download the update and restart. Version 0.4.0 and later supports the signed update feed. Older versions need the installer from this release.

Close WoW before the new companion installs or updates the addon. If automatic setup cannot find WoW, select the game folder. The separate `nauctions-ahdb.zip` is available for manual addon installation.

The installer and addon ZIP include SHA-256 files. `release-info.json` records the source revision and package hashes. The updater checks the installer signature and version.

## Validation and limits

Release checks cover Rust, frontend, addon and database tests, dependency checks, the Windows installer, signature verification, and an upgrade from the published 0.4.1 installer. Installer fixtures do not prove a live queued-upload upgrade.

Historical rise rates describe past observations. They are not calibrated probabilities or guaranteed future profit. Forecasts use asking prices, not completed sales. Missing or stale evidence is left unavailable.

Normal collection remains read-only. Shared-history writes remain disabled. Real WoW compatibility, Windows reboot behavior, sustained two-player use, and an interactive updater handoff still require real-client validation.

The installer has no Authenticode publisher certificate. Windows can show an unknown-publisher prompt. The updater signature provides a separate integrity check; no security exclusion is required.
