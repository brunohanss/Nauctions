# Nauctions 0.4.0

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
