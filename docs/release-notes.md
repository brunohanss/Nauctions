# Nauctions 0.4.1

First friend release for Windows x64, with Nauctions AHDB `0.6.0-nauctions.2`.

## Install

1. Download `Nauctions_0.4.1_x64-setup.exe` and `nauctions-ahdb.zip` from this release. The installer includes the collector.
2. Close WoW. Back up the existing AuctionDB addon folder and account SavedVariables. Extract the addon ZIP into `Interface/AddOns`. The installed path must end in `AuctionDB/AuctionDB.toc`.
3. Run the companion installer. Connect to `https://nauctions.when-to-plant.net`, choose a nickname, and join with code `COPAINS`.
4. Select the game folder containing `WTF/Account`, pair collection, and use Normal / Horde. Run an AH scan and reload or log out to save it. Confirm an accepted upload.

Both packages have SHA-256 files. `release-info.json` records their source revision and hashes. `.sig` and `latest.json` support companion updates; users do not install these files.

## Fixes and recovery

- Incomplete auction time-left data no longer causes a full-scan recording error. Missing rows remain incomplete; their values are not invented.
- API errors in later scan batches now stop scanning and report failure instead of leaving the scan stuck.
- The companion retains the audited settings backup, bounded collector logs, DPAPI credentials, durable upload queue and owned-child shutdown safeguards.
- Normal collection is read-only. Shared-history write sync remains disabled.

## Updates

Companion 0.4.0 and later can check this release through App updates. Older companions need one manual installation. The updater verifies both the installer signature and the announced version before stopping collection.

The companion updater does not replace addon files. Install the addon ZIP separately while WoW is closed. Use the two packages from this release, not an older CI artifact.

## Validation and known limits

Publication is gated on Rust, frontend, addon, database integration and dependency checks; Windows installer tests; signature verification; and a pinned 0.4.0-to-0.4.1 installer upgrade trial. The installer trial checks retained settings and opaque file fixtures, native startup, damaged-settings recovery, uninstall startup cleanup, and reinstallation. Opaque fixtures do not prove a live queued-upload upgrade.

Real WoW Forever compatibility, Windows reboot behavior, sustained two-player use and an interactive in-app installer handoff remain unverified. Perform the first scan/reload/logout trial before normal use. Synthetic tests cannot establish game compatibility.

The Windows installer has no Authenticode publisher certificate. Its updater signature is a separate integrity check. Windows may show an unknown-publisher or reputation prompt. No antivirus or Windows security exclusion is required by Nauctions.

Five informational Rust dependency warnings remain; the dependency audit reports no vulnerability failures. Asking-price forecasts and beta references are estimates, not evidence of completed sales or future profit.
