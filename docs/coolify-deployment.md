# Coolify deployment record

Date: 30 October 2026.

Status: the separate Nauctions API and database are running and healthy. Public URL: `https://api.nauctions.app`. Market: `Normal` / `Horde`. See [friend setup](connect.md).

## Separate resources

| Resource | Value |
| --- | --- |
| SSH host | `ubuntu@51.158.54.37` |
| Shared Coolify server | `jw0rnw3it1lljl1jyqzkddno` |
| Nauctions project | `xkized560ricywkoxqun1t8i` |
| Production environment ID | `2` |
| Nauctions application | `oie1i7qsa2ewyouekfkuulkk` |
| Nauctions PostgreSQL | `jjx6ddptanfpadcfhsdsd79v` |
| PostgreSQL image | `postgres:16-bookworm` |
| Database volume | `postgres-data-jjx6ddptanfpadcfhsdsd79v` |
| API image | `localhost:5000/nauctions-server:3ad4f47` |
| Image digest | `sha256:49c36afb0da86cbbac096f3e86d839ac22f2a2828b2d0b8435567ddf71610433` |
| Application source revision | `3ad4f47be2fe66914ab2bc4717b56c387d8d278f` |

The database is healthy and has no host port bindings. The `market_app` role has no superuser, database-creation, role-creation, replication, or row-security-bypass privilege. Database and application passwords and the staging key are separate generated values. They stay in root-only `/root/nauctions-runtime.json`. Do not print this file or commit its contents.

The image was built from a source-only Git archive in a separate `nauctions-builder`, limited to 2 CPUs and 4 GiB RAM. The builder was stopped after the image was loaded. The source archive, build log and protected-container baseline are under `/var/lib/nauctions-deploy`. No shared builder or cache was reset or pruned. The existing registry received only the new `nauctions-server` image repository.

## Deployment boundary

Read [the Nauctions skill](../.agents/skills/nauctions-coolify/SKILL.md). When to Plant has its own skill at `D:/Dev/when-to-plant/.ai/skills/deployment-isolation/SKILL.md`. Its agent instructions and skill index link that guard. No When to Plant application or deployment source was edited for this task.

Use `scripts/deploy-coolify.py` and `scripts/coolify_guard.py` together on the host. The helper reads `/root/nauctions-deployment.json`, verifies live ownership, and rejects the protected When to Plant IDs. It does not expose generic resource updates or shared-server changes. Run `python scripts/test-coolify-guard.py` after changes to the guard.

The helper was checked against 62 recorded When to Plant containers. Their configuration hashes and start identities matched the before-deployment record after Nauctions preparation. This checks those container records; it is not a claim about all files or background job output on the host.

## Live verification

| Check | Observed result |
| --- | --- |
| Public HTTPS | Normal certificate validation passed. TLS 1.3; Google Trust Services certificate covering the hostname. No certificate override was used. |
| API / frontend | `/healthz` returned 200. Browser frontend loaded and friend sign-in succeeded. |
| Native companion path | The actual desktop HTTP transport passed anonymous 401, friend sign-in, session reuse, market selection and pairing-code creation. |
| Windows collector | The collector extracted from the tested GitHub 0.4.0 installer paired and uploaded through the public HTTPS endpoint. |
| Receipt | Upload `dea6ad24-1238-49c1-90ce-66b8749fcf09` reached `accepted`. Accepted local payload bytes were cleared. Source SHA-256 remained unchanged. |
| Test data | One synthetic empty scan, no prices or character data. An earlier payload with no scan was correctly rejected as `no_scan_data`. |
| Cleanup | Three test identities were disabled; their devices and sessions were revoked. The synthetic empty scan was removed. Canonical scans and prices returned to zero. Audit and upload receipts remain. |
| Migrations / role | Seven successful migrations. `market_app` has none of the five privileged PostgreSQL role flags. |
| Protected application | All 62 protected container configuration/start records matched. The public When to Plant readiness endpoint returned `ready`. |
| Backups | A Nauctions-only daily backup timer is active. Its actual service run passed a shared-snapshot dump and disposable restore, including count comparison. |

The native check is `live_smoke::live_companion_sign_in_pair_and_upload` in `desktop/src/live_smoke.rs`. Normal test runs ignore it. An explicit run needs `NAUCTIONS_LIVE_SMOKE_ORIGIN=https://api.nauctions.app` and `NAUCTIONS_LIVE_SMOKE_COLLECTOR` pointing to the tested installer collector. It creates synthetic accounts and one empty scan; review and clean up only those exact test identities after a run. It does not open the GUI or change the user's existing companion profile. The browser sign-in was checked separately.

Normal desktop tests passed: 23 passed, two explicit tests ignored. Strict desktop Clippy passed. The deployment scope guard passed all five tests. These checks do not establish real-game compatibility.

## DNS and network boundary

The initially proposed `nauctions.51.158.54.37.sslip.io` address was not usable. The existing host firewall accepts public web connections only from Cloudflare. Direct requests and HTTP certificate validation timed out. No firewall rule was relaxed.

The user then approved `nauctions.when-to-plant.net`. One new proxied A record points to `51.158.54.37`. Existing When to Plant DNS records, zone TLS settings, firewall rules and application settings were preserved. Only the Nauctions Coolify route and `PUBLIC_ORIGIN` changed. The final deployment UUID is `apvcjezotquettetsdd7fzem`.

A generic Python HTTP probe received Cloudflare error 1010. The actual Rust companion transport, shipped collector and ordinary Windows curl requests succeeded. No Browser Integrity Check or other security exception was added.

## Recovery

`nauctions-backup.timer` runs daily at 03:30 UTC with up to 15 minutes of jitter. It invokes the root-owned `/usr/local/lib/nauctions/backup-coolify.py`. Its target allowlist rejects the protected When to Plant resources. The helper exports one PostgreSQL snapshot, dumps that snapshot, restores into a new random `nauctions_verify_...` database, compares counts, and removes only that disposable restore database after success.

Private bundles stay under `/var/backups/nauctions`, mode 0600. They contain the database, runtime credentials, staging key and deployment record. Do not publish them. The successful post-cleanup bundle is `nauctions-20261030T103022148301Z.tar.gz`, SHA-256 `d29d64db7c91329b73b5068c32ec28e4216dd51ce0335b8f3b07d34d286523ab`.

`scripts/pull-coolify-recovery.ps1` saves an off-server copy with Windows CurrentUser DPAPI encryption and a decrypt/hash roundtrip. Its first attempt was rejected by automatic approval review because the bundle contains runtime secrets. The user then explicitly approved this bundle's transfer to `D:/Dev/Nauctions/backups/coolify`.

The encrypted copy was saved and verified at 10:35:26 UTC on 30 October 2026 as `backups/coolify/nauctions-20261030T103022148301Z.tar.gz.dpapi`. A separate check read the saved file, decrypted it only in memory, and matched the bundle SHA-256 above. No local plaintext bundle was written. The encrypted file and verification receipt are excluded from Git. Recovery needs this Windows account and its DPAPI keys. This approval does not enable recurring transfers; recurring off-server transfer and backup retention are not configured.

The temporary token is team-scoped. Its resource restriction comes from the deployment guard, not from Coolify token permissions. The user explicitly approved temporary read/write/deploy access and revocation after deployment.

The first temporary Nauctions token was revoked at 08:54:59 UTC on 30 October 2026 while address and market choices were pending. The token used for activation was revoked at 10:31:28 UTC on the same date. Its host credential file was removed. The final verification reported both Nauctions resources healthy and all 62 protected container records unchanged. Do not reuse or replace When to Plant's credential.

## Nickname reconnection update — 2 November 2026

The live application now uses `localhost:5000/nauctions-server:ef70428`, source revision `ef7042844a5781cfcc70439fdf96d7e374010ecf`, image digest `sha256:dff5203fd576dada0adaea5eaef0aba675b80e85713df00dd1c8416c8bda5d94`. Deployment `ttweykwvmz2pynywgelgtfkx` finished. This entry replaces the initial image/revision above.

Existing active friend nicknames can reconnect with the invitation code. Public checks restored Bruno's original member ID with both Bruno and bRUNO. The two verification sessions were logged out. No replacement member was created.

The first rollout failed safely because Windows CRLF migration checksums differed from the Linux archive. The final migration runner accepts only exact LF/CRLF equivalents. Stored database checksums are unchanged. Actual SQL changes still fail checksum validation. GitHub PostgreSQL regression tests passed.

The Nauctions backup and disposable restore check passed before deployment. Final health checks passed for application, database and public /healthz. All 62 protected container records were unchanged. The temporary deployment token was revoked and removed. The isolated Nauctions builder was stopped. Companion 0.4.1 needs no reinstall. See ignored local evidence at `artifacts/reconnect-nickname/verification.md`.

## Stable server update — 2 November 2026

The live application uses `localhost:5000/nauctions-server:b28b296`, source revision `b28b296e0eca7b77290be9d543bbef5283d63e25`, image digest `sha256:58401e20bbf8956a7fc0141accac884da29eeeb9a839b92626e6f111b40bb274`. Deployment `lqvdfiwcf69bmlplbc7sl5r0` finished. This entry replaces the previous image and revision above.

The dedicated builder used 2 CPUs and a 4 GiB memory limit. It built the committed source archive. The guarded update changed only the Nauctions image. The builder is stopped. The temporary deployment token (ID 5) was revoked and its host credential file was removed.

Public HTTPS `/healthz` returned 200. Anonymous `/v1/me` returned 401. Friend sign-in with Bruno and bRUNO restored the same existing member. Both verification sessions were logged out. The application and database are healthy. All eight migrations passed. Migration 8 removed the two retired beta pricing tables. The database has no host port bindings. The `market_app` role has none of the five privileged role flags. All 62 protected container configuration and start records are unchanged. The Nauctions backup timer is active.

Host-only backups and disposable restore checks passed before and after deployment. The post-deployment backup has eight migrations and matching source/restore counts: five members, two devices, two upload receipts, and zero scans, items or observations. The bundle is `/var/backups/nauctions/nauctions-20261102T122036496996Z.tar.gz`, SHA-256 `ac104ff122c062f2654b79f1834fd249aefc25d8d4e184c3d7ef763aacea9f37`. No recovery secrets were exported.

Companion 0.4.2 was built and signed locally. GitHub publication still requires renewed GitHub authentication. Server deployment does not establish that the companion release is published. Local verification evidence is in ignored `artifacts/stable-release/server-verification.md` and `artifacts/stable-release/companion-verification.md`.

## Domain switch — 2 November 2026

The public URL is now `https://api.nauctions.app`. This replaces `https://nauctions.when-to-plant.net`. Cloudflare DNS resolves to proxied addresses. HTTPS reaches the existing Coolify proxy. Only the Nauctions application domain and `PUBLIC_ORIGIN` changed. The host deployment record now contains the new origin.

Deployment `wfb0giwgqrjeigpbodwslazj` finished. The image remains `localhost:5000/nauctions-server:b229435`, revision `b229435d09588042734757c528c95a4c9926f10e` (release 0.5.0).

The frontend and `/healthz` returned 200 with normal TLS validation. Anonymous `/v1/me` returned 401. The new origin reached invitation validation and an invalid code returned 403. The retired origin returned 403 `origin_denied`. No existing member account was accessed. Both Nauctions resources are healthy. All eight migrations passed. The database has no host port binding. The `market_app` role has no privileged role flags.

Host-only backup and disposable restore checks passed before and after the switch. The backup timer remains active. All 62 protected When to Plant container configuration and start records match the original baseline. No When to Plant DNS, application, database, firewall or proxy settings were changed. Temporary Coolify token 8 was revoked and its host credential file was removed.

Existing companion installations must use the new server address and sign in again. See [connection instructions](connect.md). Local verification evidence is under ignored `artifacts/domain-migration/`.
