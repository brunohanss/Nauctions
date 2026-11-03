# Implementation and validation status

## Release 0.6.0 preparation - 3 November 2026

This release isolates the personal watchlist, Market watchlist filter, integer coin inputs, manual notification test, and explicit gold window icon from ongoing scan/sync work. The bundled addon and collector retain 0.5.1 behavior. Automatic Windows opportunity notifications remain open. Release acceptance requires the signed installer and upgrade checks in GitHub Actions.

## Offline synchronization validation - 29 October 2026

Shared-history writes are still refused by the release collector and desktop. This is an active code restriction, not only a documentation status. The complete recovery and protocol plan remains open.

Implemented in this follow-up:

- Separate writable-target checks from read compatibility. Require the Forever root, an existing selected auction-house market, numeric item keys, all history tables, and valid dense scan rows. Refuse implicit repairs.
- Reject conflicting duplicate export identities, conflicting local identities, and incompatible daily coverage. Preserve older local history during sync. Exact repeats remain stable.
- Generate two synthetic contributor saves with auction and recipe facts. Exercise the bundled Lua initialization, history lookup, compaction, native full-scan append, and simulated serializer/reload. Parse the results with Rust and check native fingerprints, recipe facts, and the import marker.
- Extend the isolated PostgreSQL roundtrip to two members and two devices. Check shared access, canonical deduplication, contributor provenance, imported-history feedback exclusion, and new native contributions from both devices.
- Add `scripts/validate-offline-sync.ps1`, CI checks, and synthetic evidence artifacts. See [available tests and their limits](compatibility.md#tests-available-before-game-access).

These changes implement part of H06/H12. They do not complete recovery hardening, immutable export snapshots, installed-adapter verification, a real two-player pilot, or release enablement. No installed game files were used.

## Nickname onboarding and remembered access — 28 October 2026

Desktop 0.3.0 replaces ordinary member login with a nickname and the shared invitation code `COPAINS`. The server checks the code and Origin, applies a rate limit, and creates a member with a random credential. Nicknames are reserved without regard to case; the shared code cannot retrieve another user's existing identity. Administrator password access remains in a collapsed Administration section.

Migration 0004 marks remembered sessions separately from legacy password sessions. Remembered sessions have a one-year validity and renew during active use when less than six months remain. Deactivation still revokes them. The Tauri app stores its credential using Windows DPAPI and an atomic file replacement, bound to the configured server origin. It restores that credential into the native HTTP cookie jar after restart. The member credential is not returned to frontend JavaScript. Passwords and the invitation code are not saved. Explicit sign-out clears the encrypted credential.

Validation passed: 17 frontend tests, ten desktop tests, server unit tests and strict lint checks, and both PostgreSQL integration tests. Coverage includes DPAPI encryption and replacement, server binding, a new native client restoring the saved cookie, incorrect codes, nickname conflicts, simultaneous duplicate registration, CSRF, role restriction, rolling validity, and member revocation. The updated local API rejects an invalid invitation with 403. The release installer is `artifacts/desktop/Nauctions_0.3.0_x64-setup.exe`.

The local demo API was rebuilt and migrated, and the 50-item demo validation still passed. The 0.3.0 installer and checksum were generated and verified. The release app was reopened; native accessibility inspection showed Nickname, Invitation code, and Join, with no password field in the normal flow. Administration was collapsed. No real nickname or membership was created during UI review. The app was left open for the user to choose their nickname.

## Automatic Windows startup and game-aware collection — 28 October 2026

Desktop 0.2.0 adds current-user Windows startup with a hidden `--background` launch, automatic resumption of paired collectors, persistent pause controls, and game-aware file acquisition. The shared `market-game-process` crate compares process image paths with the selected WoW installation. It does not inspect game memory. A selected executable narrows matching to that file.

Collection permits a 30-second recovery window at startup and a 60-second final-save window after an active-to-closed transition. Monotonic timers prevent wall-clock changes from extending the grace period. The closed transition starts a full grace period even when a network operation delayed the next probe. Standby stops AHDB file reads but keeps upload and receipt processing active. Unknown process state is reported explicitly and keeps acquisition active conservatively.

Settings expose Start with Windows and Automatic collection, game-open state, and collector mode. Startup registration applies only to the current user. The installer removes that startup entry on uninstall. Registration is refreshed after an executable-location change, without repeatedly overriding a Task Manager disable at the same location. Signing out and Pause collection disable automatic resumption until the user resumes it.

Validation includes four game-process tests, including a live Windows image-path check against the test process and rejection of a different installation; eight desktop tests; 17 frontend tests; the full Rust workspace tests; and strict Rust lint checks. Startup tests cover paths with spaces, legacy settings, hidden launches, and owned-process shutdown. No real game process was launched for validation. The Windows installer is version 0.2.0.

The final 0.2.0 release installer and SHA-256 file were built under `artifacts/desktop`; the bundled collector hash matches the release collector. Launching the release executable with `--background` registered the quoted Nauctions command under the current user's Windows Run key. Native window inventory showed no visible Nauctions window. A manual second launch restored the main window while retaining the original process ID, 22072. The single-instance plugin also owns an internal notification window, so a nonzero `MainWindowHandle` alone is not evidence that the main application window is visible. The updated app was left open. Automatic startup is enabled for this computer; collector setup and pairing are still required. An actual Windows reboot and a real WoW session were not performed.

## Tauri desktop migration — 28 October 2026

The main Windows client is now a Tauri 2 app with a bundled React interface and collector. It has server setup, a native cookie-based API transport, local folder and executable selection, device pairing, start/stop/status/retry controls, sync actions, a tray menu, and single-instance handling. The shared API and PostgreSQL database remain central services. Browser access is retained.

Validation passed: 17 frontend tests, five desktop Rust tests, strict desktop Clippy, desktop formatting, the production frontend build, and the release NSIS installer build. The existing demo check passed with 50 items, 50 icons and listing depths, ten expected opportunities, and 473 historical points. The native HTTP test verifies cookies, Origin and CSRF headers, and redirect refusal. Process tests verify that shutdown reaps only the owned child.

Native visual review confirmed first-run setup, server configuration through IPC, a readable offline error, and the sign-in screen after the local API was restored. Closing the window hid it while the process remained running. Launching the executable again restored the same window and process. Account sign-in and real WoW file operations were not performed through UI automation. No installer was installed during this review.

The unsigned release installer and its SHA-256 file are in `artifacts/desktop`. The native app was left open, configured for the local simulated market at `http://127.0.0.1:8080`. The local PostgreSQL container and demo API were started for this preview. Production hosting, installer signing, an optional signed updater, and another-computer installation testing remain open. See [desktop setup](desktop.md).

The first Strategy tab is now implemented at `/strategy`. It adds exploratory weekly cycles, intraday recovery, dump detection, coverage heatmaps, and a whole-stack calculator with explicit cost assumptions. Read [Strategy rules and limits](strategy.md) for the current model and deferred work. It does not provide calibrated sale probabilities or verified investment returns. Automated validation includes the model, frontend calculation, and authenticated PostgreSQL read paths. Visual acceptance remains open after computer-use approval timed out.

Date: 27 October 2026. Target game build: 1.60.1.70630.

Final local checks passed: 61 Rust unit/regression tests, 11 frontend tests, and two PostgreSQL integration tests. The tests cover notification delivery and shared-history export, merge, and ingestion without a feedback loop. Rust formatting, strict Clippy, the frontend production build, and both dependency audits passed. The hosted CI workflow is configured but has not run on GitHub.

## Available software

| Backlog area | Implemented behavior | Validation |
|---|---|---|
| E01 foundations | Rust workspace, React app, PostgreSQL migrations, CI, container packaging | Workspace build, formatting, Clippy, frontend build, Docker build |
| E02 parsing | Bounded data-only Lua reader, AHDB v6 full scans, v5 metadata, recent history, separate daily imports, canonical fingerprints | Synthetic fixtures, malformed/truncated/random bytes, integer bounds, variants, order independence |
| E03 collector | Read-only multi-account discovery, double stable read, immutable SQLite queue, quota, DPAPI, pairing, receipt polling, status and heartbeat, release ZIP | Windows tests and packaged executable against the running API |
| E04 service | Invitations, secure sessions, CSRF, device grants, revocation, encrypted staging, atomic ingestion, provenance, jobs | Real PostgreSQL API tests, replay, cross-member denial, malformed source rejection, transaction rollback |
| E05 analytics | Exact min and weighted median, Paris day windows, daily-balanced baseline, coverage and freshness, bounded history | DST tests, rational extremes, incomplete coverage, complete-snapshot absence |
| E06 dashboard | Login, overview, search/sort/filter/page, item history/depth, opportunities, devices, admin | TypeScript build, frontend tests, authenticated local read API checks; visual browser review remains pending |
| E07 alerts | Owned rules/watchlists, threshold/discount/movement signals, durable dedupe, suppression, opt-in delivery | Ten repeated evaluations produce one event; stale data suppressed; local Discord mock verifies 429 recovery, concurrency, expiry, and uncertain sends |
| E08 operations | Private database Compose, Caddy TLS configuration, non-root read-only service, encrypted backup and safe restore scripts | Linux image smoke test and populated local backup/restore/replay drill |

Implementation choices:

- The collector reconciles every 10 seconds. It does not depend on filesystem event delivery.
- Daily references are calculated from immutable observations at query time. The service does not invent missing samples or mix UTC backfill rollups into Paris scan baselines.
- Prices use decimal-string rational fields at the API boundary. Browser display uses BigInt arithmetic. Chart coordinates can use approximate numbers; their tooltips keep exact values.
- Whole changed source files are uploaded without compression. The current limit is 32 MiB, with a 256 MiB per-device staging/spool limit. Real file measurements must confirm these defaults.
- A malformed or out-of-scope upload is rejected as a whole. Supported old normalized history remains readable.
- Scan completeness stays unknown unless an operator explicitly enables the tested-source policy. Matching row counts alone do not prove complete market coverage.
- The parser retains compact variant keys. Base-only AHDB history keys remain separate from variant-aware full-scan keys.
- The storage crate exposes only PostgreSQL SQLx dependencies. It avoids resolving the unused MySQL/RSA dependency tree. Register each new SQL migration in `crates/server/src/migrations.rs`; a test checks the count.
- Discord rate limits are retried. A network failure after dispatch can mean the message was accepted. Such a delivery becomes `uncertain` and is not automatically resent.
- Source account paths and character names are not stored in normalized market records. Raw encrypted staging has a seven-day retention period after terminal processing.

## Local recovery evidence

A real Windows collector read a synthetic AuctionDB file, paired with the local service, uploaded it, and observed an accepted receipt. The source hash did not change. Accepted payload bytes were removed from the local queue only after receipt reconciliation.

An age-encrypted PostgreSQL backup contained one member, one device, one upload, and three canonical scans. Restore into a new database preserved these counts. Replaying the original upload against the restored API returned accepted and left scan and alert counts unchanged. The Linux image also ran with a read-only filesystem and non-root UID, served the frontend, passed health checks, and applied the second migration on a disposable restored database.

These are local synthetic tests. They are not evidence of real game behavior, a remote production deployment, or offsite recovery.

## Open release gates

| Gate | Required evidence |
|---|---|
| E00 real source feasibility | Released client, exact AHDB release, saved full/interrupted scans, reload/relog persistence, manual price comparison, source sizes |
| E03/E09 onboarding | Two non-developer installation trials; optional startup behavior; signed installer if required by distribution policy |
| E06 visual acceptance | Desktop/mobile browser review, keyboard flow, readable chart and empty states |
| E08 production operations | Host/domain, production secrets, HTTPS certificate, scheduled offsite backup target, verified offsite restore, monitoring alerts |
| E09 pilot | Two independent accounts/computers over at least two days, real scan and alert evidence, game-file integrity, footprint measurements |
| E09 launch | Contributor consent, compatibility pin, security review, tested rollback, owner sign-off |

The game is not released and the user has no real scan files. These gates cannot be completed with synthetic data. No production credentials, real Discord messages, game actions, or production deployment were used in the implementation session.

The browser screenshot check was not completed. Automatic approval review rejected a screenshot of an existing Edge window because it could include unrelated private content. No screenshot workaround was used.

The user subsequently approved E10-06 reverse synchronization. It is implemented as an explicit Windows collector command, with an authenticated export, exact market binding, source backups, process checks, atomic replacement, and a durable import marker. AHDB itself is not modified. An optional command starts the selected executable after every selected account succeeds. Manual game launch during synchronization remains outside the collector's control; do not launch WoW in parallel.

Other post-MVP E10 work is not included: addon modifications, automated in-game actions, incremental scan extraction, auto-update, additional operating systems, multiple markets, crafting, and advanced liquidity features.

## Analytics UI revision

The approved analytics specification replaces the initial dashboard design. The default page is Opportunities. The header has two tabs: Opportunities and Items. Settings contains account, market, collector, and administration controls. The main pages use a shared search field, category filters, compact tables, and item details.

Opportunity ranking uses a backend score based on discount, reference coverage, observation age, and listed quantity. Charts show observed minimum, median, and quantity. Missing samples remain gaps. Missing icon or category metadata is not inferred. The current synthetic dataset has no qualifying opportunities.

Validation: frontend production build and 11 tests passed. The server unit suite and both PostgreSQL integration tests passed. The Opportunities and Items pages were inspected in the local browser. Mobile CSS is implemented; a separate mobile visual review remains pending.
