# Windows collector

The `market-collector` CLI reads only the selected installation's `WTF/Account/*/SavedVariables/AuctionDB.lua` files. The `run` command is read-only. The explicit `sync` command can merge shared history into these files while WoW is closed. The explicit `sync-and-start` command can then start the selected WoW executable. The collector does not scan auctions or execute Lua. The user must confirm the installation root with `pair --root`. Directory links and Windows junctions are excluded. Raw uploads can contain character names. Confirm contributor consent before pairing.

## Setup

The Tauri desktop app runs the collector with `run --game-aware`. This mode checks process image paths in the paired installation, reads AHDB saves while the game is running, allows a final-save grace period, then stops game-file reads. Durable uploads continue in standby. An optional `--game-exe` selects the exact executable. A 30-second startup check recovers a save made before the collector opened; shutdown checks continue for 60 seconds after the collector observes that an active game has closed. Unknown process state keeps collection active rather than assuming the game is closed. Local `runtime.json` reports the last game state and capture mode. Regular CLI `run` behavior stays unchanged without this option. See [desktop startup](desktop.md).

Build the executable with `cargo build --release -p market-collector`. Copy `target/release/market-collector.exe` to a directory outside WoW. Run these PowerShell commands from that directory. Replace the example paths and URL.

```powershell
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" discover --root 'C:\Games\WoW'
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" pair --root 'C:\Games\WoW' --server 'https://market.example.com' --code 'ONE-TIME-CODE'
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" run
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" status
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" retry
```

Use `run --once` for one reconciliation and upload pass. Use `--allow-localhost-http` on `pair` only for local development. Remote HTTP, URL credentials, and redirects are refused. Certificates use the standard TLS trust checks. Device enrollment calls `GET /v1/collector/capabilities`, then `POST /v1/devices/enroll`. The token is stored with Windows user-scoped DPAPI. Move or copy of the credential to another Windows user does not grant access. Pairing is supported only on Windows.

## Storage and recovery

The collector performs a full reconciliation every 10 seconds. It reads each file twice, with a 250 ms interval. It compares the hashes, file sizes, and modification times. An unstable file stays eligible for the next pass. Files over 32 MiB are deferred. Each changed stable payload gets one UUID and SHA-256 checksum. SQLite stores its immutable bytes and source checksum in one transaction. WAL mode and full synchronous commits preserve committed queue state after a restart.

The spool has a 256 MiB payload quota. It never evicts pending or unreviewed rejected bytes to make space. A quota error does not advance the source checksum. Thus, the next pass can capture the file after space becomes available. The current file can replace earlier game data before space is available; keep enough free disk capacity. Accepted payload bytes are cleared only after a receipt reports `accepted`. Accepted receipts and receipts for explicitly discarded payloads are retained for 30 days, then removed at collector startup or hourly while it runs. Legacy terminal receipts receive a full 30-day period after upgrade. Source checksums remain, so pruning receipts does not cause unchanged files to upload again. SQLite can reuse freed capacity without reducing the file size. Protect the data directory with the Windows user's normal private profile permissions.

The collector sends raw bytes to `POST /v1/uploads`. It sends `X-Upload-ID`, `X-Content-SHA256`, `X-Source-File-ID`, and a bearer token. Source IDs are SHA-256 hashes of account-relative file paths. No source paths are sent in headers. A `received` receipt causes status polling at `GET /v1/uploads/{uuid}`. A `rejected` receipt keeps the payload and error visible in `status`. Ask the server administrator to review a rejection. `retry` advances pending retry times; it does not erase a rejection or bypass an authentication pause. HTTP 401 or 403 pauses transmission and health reports until the device is paired again. HTTP 400, 409, 413, 415, and 422 mark the upload as terminally rejected, retain its bytes, and store only the HTTP status. Network and other HTTP failures keep the same UUID and bytes. Retry intervals increase exponentially with random jitter, up to 30 minutes. Compression is not enabled.

Keep the spool until all required uploads are accepted. To uninstall, stop the process, revoke the device on the server, and remove the executable and its data directory. Keep sync backups until you have checked the game data.

## Review and discard rejected payloads

Use the same data directory that the collector uses. The desktop stores one collector directory for each server under `%LOCALAPPDATA%\app.nauctions.desktop\collectors`. Check that directory's `config.json` to identify the server. Do not edit the SQLite file or delete the whole spool to free space.

1. Pause desktop collection or stop the standalone collector.
2. List rejected payloads. The output contains the upload ID, SHA-256 checksum, byte count, and rejection reason. Review the reason and decide whether the payload can be discarded.
3. To discard one reviewed payload, copy its exact upload ID and checksum into the second command. `--confirm` is required. A missing ID, a checksum mismatch, or a pending upload stops the command without discarding bytes.
4. Resume collection. Discard does not retry the rejected source. It keeps the source checksum and a discarded receipt. A later changed save remains eligible for collection.

```powershell
$collectorData = 'C:\Users\YOUR_USER\AppData\Local\app.nauctions.desktop\collectors\SERVER_HASH'
.\market-collector.exe --data-dir $collectorData rejections --limit 50
.\market-collector.exe --data-dir $collectorData discard-rejected --upload-id 'REVIEWED-UPLOAD-UUID' --sha256 'REVIEWED-SHA256' --confirm
```

The list limit must be between 1 and 500. Discard only clears one rejected payload. It does not remove queued or received uploads, pairing credentials, source checksums, game files, or sync recovery records. Keep a private copy of any payload that is still needed for diagnosis before discarding it.

## Validation limits

Automated tests use temporary directories and synthetic bytes. They include a local HTTP export, DPAPI credentials, real atomic replacement in a temporary fixture, exact backup preservation, queued original bytes, dry-run, repeat sync, invalid-market refusal, final process-guard refusal, and lock recovery. They also cover offline restart persistence, unchanged-file suppression, immutable queued bytes and UUIDs, source discovery, source modification-time preservation, overlapping writes, quota rollback, HTTPS policy, DPAPI, received/accepted polling, terminal rejection, and authentication pause. Build with scripts/collector-package.ps1 to produce a ZIP and SHA-256 checksum. Real WoW testing is pending. The reported client is **1.60.1.70630 pre-release**. No real-game save/reload, multi-PC pilot, installation trial, or compatibility result is claimed. Do not treat this CLI as a signed installer or a completed pilot release.



Local status includes the collector version, hashed source IDs, source modification time, last staging time, last accepted receipt time, acquisition errors, and next retry time. Timestamps use Unix UTC seconds. Use `heartbeat` after pairing to send a health report. This command sends queue count, version, and source file age. It does not claim an observed scan age, because the thin collector does not parse AuctionDB data.




## Optional shared-history sync

Close every WoW client first. Sync is Windows-only. Select the same realm and faction as the saved data. Use `--account ACCOUNT_DIRECTORY_NAME` to select one account. If you omit this option, sync processes every discovered account. A failed account does not roll back accounts that already succeeded. Any failure prevents game start.

```powershell
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" sync --realm 'My Realm' --house Alliance --dry-run
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" sync --realm 'My Realm' --house Alliance
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" sync-and-start --realm 'My Realm' --house Alliance --game-exe 'C:\Games\WoW\WoW.exe'
```

Sync downloads the authenticated `/v1/collector/history` export. The response has a 32 MiB limit and a 30-second timeout. The collector checks the process list before the download, file writes, replacement, and launch. A process whose name starts with `WoW` and ends with `.exe` blocks sync. A process-list error also blocks sync. Do not start the game manually during sync: process checks cannot prevent a separate program from starting immediately after a check.

Before a change, the collector queues the exact local file bytes. It creates a unique `sync-TIME-UUID.bak` snapshot in the collector data directory. It writes a temporary file beside AuctionDB.lua, flushes it, checks the source hash and modification time again, then uses Windows atomic file replacement. It does not overwrite the game's own `.bak` file. A SQLite lock prevents concurrent sync commands and is released when the process exits. The per-source journal reconciles an interrupted replacement on the next sync. If a recovery backup is missing, sync requires manual review. Keep the private data directory and its backups intact.

`--dry-run` downloads and validates the merge without game writes or backup creation. It can create collector database files. Sync requires existing AuctionDB data; it does not create an account or SavedVariables file. It can add the selected auction-house history table to an existing saved file. It keeps unrelated saved variables and existing local history. Imported history does not count as a new local scan. The source checksum is updated after replacement to prevent immediate upload of the merged file.

`sync-and-start` requires a local WoW `.exe` within the paired installation. It passes no arguments and uses no shell. It does not enter credentials or log in. A dry run never starts the game. Real-game sync, save/reload, multi-account behavior, and launcher compatibility still require a manual pilot with the reported pre-release client.

Sync requires AuctionDB **Record full scans** to be enabled (`settings.recordScans=true`). The saved sync marker identifies imported history. On a later game save, the server ignores marked imported history and daily rows for the target market. It continues to accept original full scans. This marker prevents shared history from being reported as a new contribution after a game rewrite. Legacy imports without an explicit realm are excluded from export until safe realm evidence is available.

