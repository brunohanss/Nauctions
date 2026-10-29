# Source compatibility and release gates

Target product: WoW Forever. Latest user-reported client build: **1.60.1.70685**, reported on 29 October 2026. The older **1.60.1.70630** value identifies synthetic fixtures and earlier planning; it is not an exact-build requirement. A reported version does not establish scan, profession, or save/reload compatibility.

## Build detection and compatibility policy

The addon reads the version and build through `GetBuildInfo()` when it initializes profession capture. It records both with the observations. The parser accepts the supported Forever version family and newer build numbers; it does not require an exact match with 70630. Regression tests cover 70685, a later build, and a later patch. Schema validation remains required for each upload.

A build-number change alone must not require a manual addon update. Keep build detection separate from compatibility evidence. Detect the APIs and returned data shapes that each feature needs. Use a fallback only when its meaning is implemented and tested. If a required API or schema is unsupported, pause the affected operation and show a useful status. Do not infer new field meanings from a version number, silently discard data, or mark a new build as game-tested automatically.

Profession capture starts a new local observation set when the build changes. Previously uploaded facts stay in the database under their original build. Do not relabel old facts as newly observed. This behavior does not prove that every earlier local observation was uploaded before a patch.

The planned write adapter must use a tested product/schema/API compatibility policy, not a permanently fixed numeric build. Keep the actual observed build in its evidence and diagnostics. A new build does not remove source checks, recovery requirements, or the existing release write gate. Changed installed addon fingerprints still require adapter verification.

The implementation uses the published AHDB format and synthetic test files. These tests do not prove game compatibility. Do not mark E00 or the real-client pilot complete until real evidence exists.

Full scan format: version 6. Item metadata: version 5. Source references:

- https://github.com/mooreatv/AuctionDB/blob/master/AuctionDB/Record.lua
- https://github.com/mooreatv/AuctionDB/blob/master/AuctionDB/History.lua

Recorded row counts can detect some partial scans. Matching counts do not prove complete market coverage. The default parser therefore marks these scans as unknown. Enable trusted complete scans only after a live full-scan and interrupted-scan comparison proves this policy for the pinned addon version.

Before release, record the AHDB commit, realm, house, file size, save duration, and fixture SHA-256. Verify reload, logout, restart, two account paths, interrupted scans, bid-only listings, item variants, and manual prices. Obtain contributor consent before retaining any raw sample. Remove player identifiers before committing fixtures.

Real-client validation, two-player multi-day testing, production hosting, and owner launch sign-off remain external release gates. Continue software implementation and synthetic validation while these gates are pending.

Reverse synchronization has a legacy implementation, but release commands refuse writes in both the collector and desktop. The recovery and protocol work in [the hardening plan](ahdb-sync-hardening-plan.md) remains required. Preview and read-only collection remain available. Do not remove the write gate to test an installed game. No real game file has been changed during development.

## Tests available before game access

Run `./scripts/validate-offline-sync.ps1` from PowerShell. Install the pinned Python requirements into `artifacts/lua-test-runtime` first if they are absent. The script uses only synthetic files under the repository. It does not read an installed game's files or start the game.

The checks generate separate synthetic saves for two contributors, merge an explicit shared history bundle with Rust, and load each result through the bundled addon initialization, history, and full-scan recording code in Lua 5.1. They check numeric item lookup, recent and daily history, typical prices, compaction, a new native scan, a simulated save/reload, recipe preservation, and the import marker. Rust then parses both simulated saves and verifies native scan fingerprints and shared recipe facts. The tests retain addon hashes and output hashes in `artifacts/offline-sync/evidence.json`.

This is a two-contributor simulation, not the real two-player pilot. The serializer and game API stubs are test code. They do not prove that the actual client loads the addon, exposes the expected APIs, saves the same data, or preserves the marker.

| Environment | Useful evidence | Cannot establish |
| --- | --- | --- |
| Synthetic Lua harness | Bundled addon logic and Rust roundtrip | Target-client API and save behavior |
| Disposable Windows fixtures | Collector queue, process checks, file replacement and recovery tests that exist | Complete recovery hardening or actual power-loss durability |
| Isolated PostgreSQL tests | Ingestion, replay, feedback exclusion, market scope and recipe conflicts | Real scan completeness or real recipe values |
| Another Classic client | Only behavior explicitly exercised on its recorded build and matching code path | Forever compatibility, Forever auction/profession behavior, or the release gate |
| Exact target client | Actual API, scan, tooltip and save/reload behavior | Two-player reliability until the multi-day pilot also passes |

The upstream author maintains the Forever edition separately from the Classic/Mists/retail legacy code: see the [upstream README](https://github.com/mooreatv/AuctionDB#readme). Treat an alternate-client pass as limited smoke-test evidence. An alternate-client failure can also result from an API difference rather than a Forever defect. Keep its files and database market separate. Do not change the Forever adapter or promote scan completeness merely to make that test pass.

The next target-client session must record `GetBuildInfo()` output, installed addon hashes and `.toc` metadata, then exercise native scans, professions, tooltip history, reload, logout and restart. Complete read-only checks first. Enable write tests only after the software recovery gates pass, and use copied test files with verified originals. Record actual observed results in `release-validation.md`; retain every unrun gate as pending.

## Pinned upstream schema reference

On 27 October 2026, the GitHub repository API resolved `master` to commit `9a92cb047819fbf35219df58ef5dd3e6a28602c4` (25 October 2026, 20:13:49 UTC). The parser reference is this commit, not a moving branch:

- [Record.lua](https://github.com/mooreatv/AuctionDB/blob/9a92cb047819fbf35219df58ef5dd3e6a28602c4/AuctionDB/Record.lua)
- [History.lua](https://github.com/mooreatv/AuctionDB/blob/9a92cb047819fbf35219df58ef5dd3e6a28602c4/AuctionDB/History.lua)
- [AuctionDB.lua](https://github.com/mooreatv/AuctionDB/blob/9a92cb047819fbf35219df58ef5dd3e6a28602c4/AuctionDB/AuctionDB.lua)

The files were read at the pinned commit through the GitHub connector. `AuctionDB.lua` identifies `AuctionDBForeverSaved` as the SavedVariables global. Its history houses contain no realm field. A configured and verified server market must supply that missing context. History uses base item IDs and does not prove variant-level prices.

This pin proves the schema source used for implementation. It does not prove that the target client loads the addon or writes this format. Record the installed release separately when the real-client gate can run.
