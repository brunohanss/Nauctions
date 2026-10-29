# Release validation

Record the date, Git revision, Windows version, installer SHA-256, operator, and result for each trial. A trial passes only when the observed result matches the expected result. Retain failure details. Do not mark a pending trial as passed.

## Automated checks

The `checks` workflow packages `nauctions-ahdb.zip` and its checksum. Packaging verifies every archive entry against its source and requires the `AuctionDB/` install layout, upstream licence and source notices. The `desktop` workflow builds the companion installer with its bundled collector.

After both workflows pass for the candidate, run `prepare desktop release` manually. It uses `scripts/release-package.ps1` to build both packages, records both versions and hashes in `release-info.json`, verifies the signed installer, and creates a draft release at the tested revision. The draft includes the addon ZIP, companion installer, checksums, installer signature, updater manifest and release record. It does not publish or deploy the backend.

Run the `checks` and `desktop` GitHub workflows. Both must pass for the release revision. The release preparation workflow runs both again before it creates a signed draft release.

The checks cover Rust tests and lint, frontend tests and build, PostgreSQL integration, dependency audits, Windows collector tests, and desktop tests. Updater verification tests reject changed bytes, an invalid key, and a manifest version that differs from the signed version.

The PostgreSQL integration tests also place malformed metadata before a valid upload. They verify a rejection receipt, rollback of partial writes, retained staged bytes, and acceptance of the next upload. A simulated connection error must leave valid data available for retry.

The desktop workflow also installs the package on its disposable Windows runner. It verifies the installed collector hash, same-version reinstallation, saved preferences, native startup, and recovery after an interrupted settings write. This smoke test does not replace a version-to-version upgrade or an in-app update trial. The script refuses to run outside GitHub Actions.

The release workflow verifies the actual installer against the public key shipped in the app before creating a draft. The verification checks the manifest version, URL, signature, changed bytes, and a changed version. This test does not install an update.

## Installation and upgrade trial

Use two Windows computers or clean Windows virtual machines. Use separate Windows accounts and a disposable market. Use synthetic game files. Do not test a write operation on the only copy of a game file.

1. Install 0.3.1. Join the test market, select a test game folder, and pair the collector.
2. Disconnect the API. Queue a synthetic save. Record the source hash and pending upload UUID.
3. Quit Nauctions from its tray. Install 0.4.0 in the same location. Open it.
4. Confirm that the server address, nickname, selected paths, startup preference, and pairing are retained. Confirm that the pending UUID and payload are retained.
5. Restore the API connection. Confirm one accepted scan and no duplicate contribution. Confirm that the source hash did not change.
6. Restart Windows. Confirm one app process, the selected startup behavior, and the saved collection preference.
7. Prepare a signed higher-version test release. Use a separate test manifest endpoint and test signing key for the trial build. Do not publish a test manifest to the stable endpoint.
8. Select **Check for updates**. Confirm the version and notes. Start the download. Confirm that collection continues during download.
9. Install the update. Confirm that the owned collector stops before installation, the app restarts, and the values from step 4 remain correct.
10. Repeat with an interrupted download, an invalid signature, and an unavailable endpoint. Confirm a readable error and continued collection. Confirm that the old app remains usable.
11. Run a sync preview while installation is requested. Confirm that installation waits for a later user retry and does not interrupt the collector action. Confirm that the desktop and collector refuse write sync. When writes are enabled in a later release, repeat with a disposable sync write and verify its backup and recovery.

Status: pending. Building the installer and passing unit tests do not complete this trial.

## Sustained synthetic trial

Run two collectors for 48–72 hours against a disposable database. Replay irregular saves. Include duplicate files, truncated files, concurrent file writes, service restarts, network loss, and collector restarts. Keep real Discord delivery disabled.

Pass criteria:

- Valid uploads reach an accepted receipt after connectivity returns. Invalid uploads have a visible rejection reason.
- Replayed uploads do not increase canonical scan counts or repeat alerts.
- Pending payloads remain recoverable after a process restart.
- Read-only collection does not change source hashes.
- Queue growth stays within the configured quota. Quota errors remain visible.
- Record CPU, memory, disk, API latency, and oldest pending upload age throughout the trial. Investigate continued growth after the workload becomes idle.

Status: pending. This trial needs elapsed runtime and measured results.

## Production and game gates

Before launch, verify HTTPS, member revocation, monitoring, a scheduled offsite backup, and a restore into a separate database. Check schema compatibility before a server rollback. Test an older supported desktop against each new API.

When the target client is available, complete `compatibility.md` with real full scans, interrupted scans, reload/logout behavior, manual price comparisons, and two independent players over at least two days. Keep scan completeness untrusted until this evidence exists. Keep shared-history writes disabled during initial compatibility validation.

Status: pending. Synthetic data cannot close these gates.

Shared-history writes are disabled in 0.4.0. Resolve and validate the findings in `ahdb-sync-audit.md` before restoring this feature. Passing the current merge tests does not close those findings.
