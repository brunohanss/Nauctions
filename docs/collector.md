# Windows collector

The `market-collector` CLI reads only the selected installation's `WTF/Account/*/SavedVariables/AuctionDB.lua` files. The `run` command is read-only. The explicit `sync` command can merge shared history into these files while WoW is closed. The explicit `sync-and-start` command can then start the selected WoW executable. The collector does not scan auctions or execute Lua. The user must confirm the installation root with `pair --root`. Directory links and Windows junctions are excluded. Raw uploads can contain character names. Confirm contributor consent before pairing.

The Nauctions AHDB fork also saves shared profession facts in this file. The same
collector path uploads both sections. Recipe facts contain no character profile or
learned-recipe state. See [crafting setup](crafting.md).

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

Use `run --once` for one reconciliation and upload pass. Use `--allow-localhost-http` on `pair` only for local development. Remote HTTP, URL credentials, and redirects are refused. Certificates use the standard TLS trust checks. Device enrollment calls `GET /v1/collector/capabilities`, then `POST /v1/devices/enroll`. The token is stored with Windows user-scoped DPAPI. SQLite stores the protected credential and pairing configuration in one transaction with the pending queue transition. Compatibility copies remain available in `config.json` and `credential.dpapi`, but SQLite is authoritative after pairing. Re-pairing gives each retained queued or received payload a new upload ID and sets it to queued. The server deduplicates canonical scan content. Move or copy of the credential to another Windows user does not grant access. Pairing is supported only on Windows.

## Storage and recovery

The collector performs a full reconciliation every 10 seconds, including in standby. A separate worker sends one pending request per cycle, with a 5-second request timeout. Slow transport does not stop file acquisition. All commands hold an exclusive data-directory lock. The desktop owns its running collector through a Windows Job Object; closing the process handle on desktop termination also terminates the collector. It reads each file twice, with a 250 ms interval. It compares the hashes, file sizes, and modification times. An unstable file stays eligible for the next pass. Files over 32 MiB are deferred. Each changed stable payload gets one UUID and SHA-256 checksum. SQLite stores its immutable bytes and source checksum in one transaction. WAL mode and full synchronous commits preserve committed queue state after a restart.

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



Local status includes the collector version, hashed source IDs, source modification time, last staging time, last accepted receipt time, acquisition errors, and next retry time. Timestamps use Unix UTC seconds. The running collector sends a bounded health report every five minutes. Use `heartbeat` to send one manually. This command sends queue count, version, and source file age. It does not claim an observed scan age, because the thin collector does not parse AuctionDB data.




## Shared-history preview and local recovery

Shared-history apply and sync-and-start remain disabled in release commands. The collector and desktop enforce this limit. Normal collection and uploads continue. The implementation and remaining release checks are described in [the hardening update](ahdb-sync-hardening-update.md).

Select one account explicitly. Run `discover` to get the local account paths. Check the realm and auction house for that account. An omitted `--account` stops preview. Preview checks the exact bundled addon fingerprints and refuses other versions or modified files. It does not repair missing house tables.

```powershell
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" discover --root 'C:\Games\WoW'
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" sync --realm 'My Realm' --house Alliance --account 'ACCOUNT_NAME' --dry-run
```

Preview uses history protocol 2. The server returns one complete immutable response from a repeatable-read database transaction. The collector checks its digest, record counts, market, timestamps, expiry, and record limits before it merges data. The legacy endpoint returns an upgrade-required response to old collectors. Unknown daily coverage overlap stops the operation. It does not select an alternative by scan count or add unknown counts.

The installation coordinator is `WTF/.nauctions-sync/`. It contains an exclusive Windows lock, an installation identity, an operation journal, and operation manifests. The lock applies across collector data directories. A read-only collector does not create this directory. When a coordinator exists, collection uses its lock for capture and checksum changes. Pending uploads can continue while capture is deferred.

Local recovery does not need a server, pairing, or a device token. Close all WoW clients first. Use **Local recovery · works offline** in the desktop connection area, including before sign-in. Inspect records before you select a restore. The CLI has the same actions:

```powershell
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" recovery-status --root 'C:\Games\WoW'
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" recover --root 'C:\Games\WoW'
.\market-collector.exe --data-dir "$env:LOCALAPPDATA\Nauctions" restore --root 'C:\Games\WoW' --operation 'OPERATION_UUID' --expected-current-sha256 'HASH_FROM_PREVIEW'
```

Use `missing` as the expected hash only when preview reports a missing source. Automatic recovery restores a verified original only when the source is missing. It keeps an unexpected current file and reports review required. Explicit restore requires the exact current hash shown in preview. It backs up that current file and journals the restore before replacement. A changed hash stops restore.

Before replacement, the internal writer keeps a verified private original, a verified original beside the source, and a verified candidate. Windows receives a separate unique backup path. None of these files use the game's `AuctionDB.lua.bak` name. After a replacement attempt, the writer checks the actual source state even when Windows reports failure. It retains recovery artifacts. It never truncates the source as a fallback.

Keep the installation coordinator and the private backup directories. There is no automatic backup cleanup. New operations stop at a 1 GiB retained-artifact budget or insufficient free space. A legacy journal with no recoverable account path stops with a manual-review error; the tool does not guess the target or delete the record.

The writer requires a local NTFS installation and refuses links, junctions, network paths, unsupported addon files, and unknown game-process state. A five-second stable-file gate reduces late-save races. It cannot prevent a separate launcher or user from starting WoW after a process check. Do not start the game during recovery or restore.

No real game file was used for the synthetic checks. Real-client save/reload, the two-computer pilot, forced-reset tests, disk-full tests, and second-Windows-user access remain release gates. Shared-history writes stay disabled until that evidence is available.
