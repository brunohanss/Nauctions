# Nauctions 0.4.0

This candidate is for pre-launch testing. Real-client compatibility and production acceptance remain pending.

## Downloads

- `nauctions-ahdb.zip`: addon `0.6.0-nauctions.1`. Close WoW and extract into `Interface/AddOns`. The ZIP contains the `AuctionDB` folder, source files, licence and source notices. Back up upstream AHDB and SavedVariables before replacing the addon.
- `Nauctions_0.4.0_x64-setup.exe`: Windows x64 companion with its collector. Run the installer, configure the shared API, join the group, select the game folder, and pair collection.
- Both downloads include a SHA-256 file. `release-info.json` identifies their source revision, versions and hashes. The installer signature and `latest.json` support verified companion updates.

## Included changes

- The addon captures profession facts and auction recipe discovery. It adds crafting cost estimates, a separate attributed beta reference, and expiring forecast-cache support.
- The companion adds Crafting, Forecasts, and Help and diagnostics. Forecasts and asking-price estimates do not prove future sale prices or realized profit.
- Collection now owns its child process with a Windows Job Object. Re-pairing recovers retained uploads, and stalled uploads do not block file acquisition. Status shows authentication, acquisition and queue failures.
- Market freshness expires from observation time. Browser requests time out and offer retry. Alert edits cannot restore deleted rules, and notification opt-out cancels pending delivery.
- Browser and native telemetry exclude sensitive URL and path details. Native queued work is discarded after opt-out. Support exports and dependency checks use the patched ZIP dependency.

## Companion installation and updates

- The app checks for updates at startup. Select **Download update and restart** to install an available update.
- Updates require HTTPS and a valid signature from the Nauctions release key. The signature must match the announced version.
- The app downloads and verifies the update before it stops its collector. Settings, remembered membership, pairing, and pending uploads stay in the application data directory.
- Windows CI now runs collector and process-detection tests. Dependency checks include the desktop lock file.
- The API rejects metadata that PostgreSQL cannot store, rolls back partial changes, and continues with the next upload. Deploy the updated backend to apply this fix. Operational database errors remain available for retry.
- Desktop settings use atomic saves and a valid backup. Startup restores damaged settings when possible. If both copies are damaged, it preserves them and disables automatic collection and Windows startup.
- Collector logs rotate while the collector runs. The desktop keeps the current log and one previous log, each at most 5 MiB.
- Collector commands can list rejected uploads and discard one reviewed payload by upload ID and checksum. Pending uploads remain protected. Old accepted and explicitly discarded receipts are pruned after 30 days.
- Shared-history writes and Sync and start are disabled in the desktop and collector command. Read-only collection and sync preview remain available. The existing recovery defects in `docs/ahdb-sync-audit.md` must be resolved before writes are enabled.

Users of 0.3.1 and earlier must install this version from its setup executable once. Future releases can use the update command.

Game compatibility is still unverified. Synthetic tests do not prove compatibility with the released WoW Forever client. Shared-history sync and installer upgrades still require the trials in `docs/release-validation.md`.
