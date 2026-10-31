# Friend installation audit — 31 October 2026

## Decision

Do not describe this candidate as a proven, maintenance-free release. The available checks support a supervised first installation. They do not establish real-game compatibility or a complete signed upgrade cycle. Complete the short game trial below before leaving the installation in normal use.

No computer-damaging operation was found in the normal companion collection path. This is a code-review result, not a guarantee that every Windows environment or game client is compatible. The collector reads SavedVariables and process image paths. It does not inject code into WoW, read game process memory, install a driver, or require a machine-wide startup service. The installer uses current-user installation. Uninstall removes the Nauctions current-user startup value.

## Candidate identity

- Base revision: `3ad4f47be2fe66914ab2bc4717b56c387d8d278f`.
- Companion: `0.4.0`, from the successful [desktop CI run](https://github.com/brunohanss/Nauctions/actions/runs/40956141088).
- Exact local copy of that installer: `artifacts/deployment/ci-installer/Nauctions_0.4.0_x64-setup.exe`.
- Installer SHA-256: `9e4bd23de22fbe189ab3a670fc033cd2d78d35e95af3afdecf73b3047a64a3ac`.
- Extracted CI collector SHA-256: `060974d9b91e2007a868d467474988ac98104571f4e107a6be5e9c4ef1e70705`.
- Patched addon: `0.6.0-nauctions.2`, built from this working tree. It includes the two fixes below. It is newer than the addon in the existing CI download.
- Patched ZIP: `artifacts/friend-audit-2026-10-31/nauctions-ahdb.zip`.
- ZIP SHA-256: `e7f3bdb77170bf8f1cadd33aeb330549ca8c4d1c2d9aaa1e90e4474f2468026c`.

The working tree already contained deployment documentation and a test-only native smoke module before this audit. They were preserved. This audit did not rebuild the companion, install it over the user's profile, publish a release, or change the production service.

## Findings and repairs

| Priority | Finding | Result |
| --- | --- | --- |
| P1 | Full-scan recording performed arithmetic on a missing time-left value. An incomplete replicate row with a valid item link produced a Lua exception. | Fixed in `Record.lua`. Missing, fractional, negative and out-of-range time bands are excluded. Expected scan count remains unchanged, so missing rows do not become evidence of completeness. New regression passes. |
| P1 | An API exception in a later 2,000-row scan batch escaped the event handler and left `scanState` at `reading`. The user could be forced to reload the client to scan again. | Fixed in `Scan.lua`. Each timer batch, initial read and replicate request handles an exception through `ScanFailed`. A regression throws in row 2,001, confirms that scanning stops, and confirms that no full scan is saved. |
| P1 release gate | Real-client load, auction, tooltip, profession and save behavior is not established by the repository evidence. | Open. The Lua harness uses mocked APIs. Do the game trial below. |
| P1 release gate | The stable updater endpoint has no published GitHub release. The existing installer/update trials do not prove an in-app version upgrade. | Open. GitHub releases returned an empty list during this audit. Do not promise automatic updates until a signed higher-version release and upgrade trial succeed. |
| P2 | Multiple local installers have the same `0.4.0` filename. The older `artifacts/desktop/release-info.json` records revision `fc73fdb`, not the later CI candidate. | Use the exact CI artifact and hash above. Do not choose an installer by version number alone. |
| P2 | The CI installer has no Windows Authenticode publisher signature. | Verified with `Get-AuthenticodeSignature`: `NotSigned`. A Tauri update signature is a separate mechanism. Windows reputation or security software can affect installation; the friend's exact policy was not tested. |
| P2 maintenance | The companion updater does not update `Interface/AddOns/AuctionDB`. | Future addon fixes require replacement of addon files while WoW is closed. There is no verified zero-maintenance update path for both products. |
| P2 capacity | Real-market frame time, SavedVariables size and long-run resource use remain unmeasured. | The collector rejects files over 32 MiB and limits retained payload bytes to 256 MiB. These limits protect collection but can stop new uploads. The payload quota is not a hard limit on all SQLite files or all app data. |

The repairs are local and packaged. The old CI addon does not contain them. No SavedVariables schema or companion API change was made.

## Verified safeguards

| Area | Evidence and practical limit |
| --- | --- |
| Game files | Discovery and stable-read tests preserve source bytes. Normal collection does not write the game files. Both companion and collector refuse shared-history write sync. Explicit recovery/restore commands are separate write operations; they are not needed for the friend pilot. |
| Shutdown | Owned-child stop/reap and forced-parent-termination tests pass. The Windows Job Object terminates the owned collector if the companion dies. No broad process-name kill is used. |
| Startup | Executable paths with spaces are quoted. Registration is current-user only. The tray provides Quit; closing the window keeps collection active. |
| Settings | Atomic settings writes and backup recovery pass. Damaged files are preserved. With two invalid settings copies, automatic collection and startup default to disabled. |
| Credentials | Windows DPAPI roundtrip and remembered-session/server-binding tests pass. Saved pairing and queue transitions use a SQLite transaction. |
| Network loss | Pending bytes and upload IDs survive restart. Receipt validation, backoff, authentication pause and re-pair recovery tests pass. A missing game folder does not prevent spool-only recovery. |
| Local resource use | Collector output rotates into two bounded 5 MiB logs. Logging failure continues to drain child pipes. Parser input and nesting limits, malformed input tests and upload quotas pass. |
| API access | The native transport requires HTTPS except explicit local development. Redirects are disabled. Request paths cannot escape the selected API origin. Response size and request duration are bounded. |
| Update integrity | Tests reject changed bytes, wrong signing keys and a version not bound to the signature. Download verification occurs before collection stops. This is not a completed installed upgrade trial. |
| Data quality | Expected row counts survive missing rows; unresolved item links do not become invented base-item records. Keep completeness untrusted and shared-history writes disabled. |

## Checks run in this audit

| Check | Result |
| --- | --- |
| Rust workspace tests | 130 passed; 3 PostgreSQL integration tests ignored by this command |
| Desktop tests | 23 passed; live-account mutation and actual signed-release verification tests not run |
| Workspace and desktop strict Clippy | Passed; local incremental-cache hard-link warnings only |
| Workspace and desktop formatting | Passed |
| Frontend tests | 31 passed |
| Frontend production build | Passed; existing large JavaScript chunk warning remains |
| Addon Lua 5.1 harness | 24 passed, including both new regressions |
| Forecast Lua harness | 5 passed |
| Synthetic two-contributor Lua history roundtrip | 1 passed |
| Addon packaging | 24 archive entries verified against exact source bytes and install layout |
| Fresh npm advisory audit, including development dependencies | Zero vulnerabilities |
| Fresh Rust workspace advisory audit | Zero vulnerabilities, no warnings |
| Fresh desktop advisory audit | Zero vulnerabilities; five informational warnings remain |
| Git diff whitespace check | Passed |

Desktop warnings: `proc-macro-error` (RUSTSEC-2024-0370), `zbus_names` (RUSTSEC-2026-0371), `zvariant` (RUSTSEC-2026-0370), `zvariant_derive` (RUSTSEC-2026-0372), and `glib` (RUSTSEC-2024-0429). The `glib` warning is unsoundness, not merely maintenance status. The Windows-target reverse dependency check contains no `glib` package. No advisory exception was added. Audit JSON is retained under `artifacts/release-audit/friend-*-audit.json`.

An additional synthetic recording probe used 85,000 auction rows and 20,000 item IDs. Four recordings retained exactly three scans, with 4,022,997 bytes of listing strings. Fixture creation and four recordings took 1.573 seconds in local Lua 5.1; sampled Lua heap was approximately 50 MiB. This does not measure game frame time, the full history/profession workload, full serialized file size, or the friend's PC.

## Existing CI evidence checked again

The [desktop run](https://github.com/brunohanss/Nauctions/actions/runs/40956141088) succeeded at the base revision. Its clean Windows install/start step succeeded. The script covers fresh install, bundled collector hash, same-version reinstall, retained settings, native startup, and damaged-settings recovery. It does not cover uninstall, Windows reboot, a version-to-version upgrade or an actual in-app update.

The [checks run](https://github.com/brunohanss/Nauctions/actions/runs/40956141137) also reports success for dependency audit, addon, Windows collector, frontend and Rust jobs. These runs precede today's addon fixes.

The local Docker engine was unavailable. The three PostgreSQL integration tests were not repeated today. No production database was used as a substitute.

Public HTTPS checks today returned 200 for the frontend and collector capabilities, and 401 for an unauthenticated `/v1/me`. `/healthz` returned 200 with `status: ok`. Capabilities confirmed `history_writes_enabled: false` and a 32 MiB upload limit. `/health` is a frontend fallback and must not be treated as backend health evidence; use `/healthz`. Public reachability does not prove backups, restore readiness, authenticated uploads, host capacity or monitoring. The earlier native sign-in/pair/upload evidence in `connect.md` is dated 30 October and was not repeated with a new live account today.

## Installation and acceptance on the friend's PC

1. Close every WoW client. Back up the existing `Interface/AddOns/AuctionDB` folder and each account's `WTF/Account/<account>/SavedVariables/AuctionDB.lua` and `.bak`, if present. Store the backup outside the game folder.
2. Use the exact companion artifact and patched addon identified above. Check their SHA-256 values. Keep the addon folder name `AuctionDB`; do not enable a second upstream AHDB copy.
3. Install the companion as the friend's ordinary Windows user. Open it, join the configured server, select the game folder that contains `WTF/Account`, and pair collection. Confirm the selected market is Normal / Horde.
4. Start the target game. Record its `GetBuildInfo()` output and addon version. Check login and reload for Lua errors. Open item tooltips, the AH scan control, the history graph, and the profession/crafting view.
5. Run one full scan. Close the AH during another attempted scan when permitted. Confirm that a failure clears the scanning state and that the game remains usable. Check frame responsiveness during a representative large scan.
6. Use `/reload` or log out. Confirm that the companion reports an accepted upload and that the expected market data appears. Do not use a running collector or a recent file timestamp alone as proof of success.
7. Disconnect the network, save another scan, quit and reopen the companion, then reconnect. Confirm that retained uploads complete and that settings remain available.
8. Quit from the tray and verify that the collector stops. Restart Windows once. Confirm the chosen startup preference and only one companion/collector instance.
9. If any game-facing step fails, disable only AuctionDB in the game's addon list and quit Nauctions from the tray. Preserve diagnostics and saved files. A complete game reinstall is not the first recovery action.

Keep the full two-player and sustained-runtime trials pending until they are actually performed. Complete a higher-version signed update trial before depending on unattended future companion updates. These are the remaining checks that address the user's concern about discovering a repair or reinstall requirement after leaving the friend's PC.
