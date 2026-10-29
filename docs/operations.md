# Service operations

This repository provides deployment files. It does not prove a production deployment or a real WoW pilot. The source feasibility gate remains open for WoW Forever client 1.60.1.70630 pre-release.

## Start a local database

Install Docker Desktop and select Linux containers. Run `./scripts/dev.ps1 -DatabaseOnly` in PowerShell. The script creates a random password in ignored `data/dev-postgres-password.txt`. The PostgreSQL 16 service listens only on `127.0.0.1:55439`. Its database and user are `nauctions`. Do not publish port 55439 on a public interface. The password file is a local development credential; keep the directory private.

Set `DATABASE_URL` from that file when you run tests in another shell. Do not print the URL into a shared log:

```powershell
$pgSecret = [IO.File]::ReadAllText((Join-Path $PWD 'data/dev-postgres-password.txt')).Trim()
$env:DATABASE_URL = "postgresql://nauctions:${pgSecret}@127.0.0.1:55439/nauctions"
cargo test --workspace --locked
```

For a local server, set `MARKET_REALM`, `MARKET_HOUSE`, `STAGING_KEY_HEX`, `BOOTSTRAP_ADMIN_LOGIN`, and `BOOTSTRAP_ADMIN_PASSWORD`. Then run `./scripts/dev.ps1`. The script sets the local origin and binds the API to `127.0.0.1:8080`. Build the web client with `npm ci` and `npm run build` in `web` before you start the server. The server serves `web/dist`.

## Configure the host

1. Create a host with Docker Engine and Compose. Permit inbound TCP 80/443 and UDP 443. Do not expose PostgreSQL or port 8080.
2. Point one DNS hostname to the host. Copy `deploy/example.env` to `deploy/.env`. Keep `.env` readable only by the operator.
3. Generate separate random values for `POSTGRES_PASSWORD` and `APP_DB_PASSWORD`. Use at least 32 random bytes encoded as hex for each. Hex avoids URL encoding errors in `DATABASE_URL`. The backend uses `market_app` with the application password. Keep the maintenance password out of the backend configuration.
4. Generate a separate 32-byte random hex value for `STAGING_KEY_HEX`. Save the key in the secret manager and the recovery record. The database backup alone cannot recover encrypted raw staging data without this key.
5. Set the real market realm and house. Set a unique admin login and a long random admin password. Set the hostname and ACME email. Do not use example credentials.
6. Keep `TRUST_RECORDED_SCAN_COMPLETENESS=false` until real-source validation proves the recording semantics. Set a private Discord webhook only when alert delivery is approved.
7. Run the commands below from the repository root. The first command validates configuration without printing resolved secrets.

```powershell
docker compose --env-file deploy/.env -f deploy/compose.yml config --quiet
docker compose --env-file deploy/.env -f deploy/compose.yml up -d --build --wait
docker compose --env-file deploy/.env -f deploy/compose.yml ps
```

Caddy obtains and renews the public TLS certificate. The backend has no host port. PostgreSQL is on an internal network. On first initialization, `deploy/initdb/010-market-app.sh` creates `market_app` without superuser, database-creation, role-creation, replication, or row-security-bypass rights. The application owns the public schema and can create its migration tables there. It can connect to its database but does not own the database. The `nauctions` maintenance role owns the database and remains available for backup and restore. The backend runs as a non-root user with a read-only filesystem. The service runs embedded SQL migrations at startup. The Docker build compiles the Rust server and the React static bundle. The container serves the bundle through the authenticated service routing rules.

The compose files use image version tags. Before production release, record the tested image digests, the Git revision, the dependency lockfiles, and the deployed migration version. Review changes before updating images. This repository does not claim image signature verification or a signed Windows installer.

## Health and monitoring

Check `https://YOUR_HOST/healthz` after startup. Check the authenticated dashboard, device enrollment, upload receipt, and one item history with an authorized test user. A green HTTP health check does not prove ingestion, alert delivery, or source compatibility.

Use `docker compose --env-file deploy/.env -f deploy/compose.yml logs --tail 100 backend` for service diagnostics. Keep operational logs private. Do not log bearer tokens, passwords, webhook URLs, raw files, or account paths. Track failed logins, upload rejection codes, pending receipt age, staging disk use, database growth, backup age, and offsite verification failures. Add external HTTPS uptime monitoring on the production host.

## Encrypted backups

Install `age` and `rclone` on the operator machine. Create an age recipient and keep its private identity in a separate secret store. Configure an authenticated rclone remote with private storage. Do not store the age identity in the repository or inside the backup archive. Use encrypted local storage because backup and restore briefly create plaintext dump files. Both scripts remove their temporary plaintext files in `finally` blocks. An abrupt power loss can leave a temporary file; inspect the private backup directory after recovery.

```powershell
./scripts/backup.ps1 -AgeRecipient 'age1YOUR_PUBLIC_RECIPIENT' -OffsiteRemote 'private-remote:nauctions/backups'
```

The script creates a PostgreSQL custom dump, encrypts it, records the encrypted SHA-256 checksum, uploads the encrypted file and checksum, and compares downloaded offsite bytes. The script stops if any step fails. It never sends a plaintext dump offsite. Use `-LocalOnly` only for an explicit local test. Use `-ComposeFile deploy/compose.dev.yml` to back up the local database; set `POSTGRES_PASSWORD` from the local password file first.

Schedule this command at least daily on the operator host. Alert when the last verified backup is older than 26 hours. Initial retention: 14 daily copies and 8 weekly copies. Configure retention on the storage service after confirming the recovery requirement. This script does not delete old backups automatically. Record the backup filename, time, deployment revision, migration count, upload count, scan count, and recovery-key location in the private operations record. Check the database counts before and after the dump during a maintenance window if an exact comparison is required.

## Restore drill

Run a restore drill before launch and after a migration that changes ingestion or deduplication. Download the encrypted backup and its `.sha256` file. Start the local PostgreSQL service, then run:

```powershell
./scripts/restore.ps1 -BackupFile './backups/FILE.dump.age' -AgeIdentityFile 'C:\Private\nauctions.agekey' -TargetDatabase 'restore_drill_20261027'
```

The target must be a new database. The script refuses the live database names and never drops a database. It validates the checksum, decrypts the archive, creates the target, restores with error checking, and reports migration/upload/scan counts and unvalidated foreign keys. Compare counts with the private backup record. Point a separate test server at the restored database. Keep production ingestion and Discord delivery disabled in that test server. Replay a known upload with the same device and upload UUID; confirm unchanged accepted scan counts and no repeated alert. Confirm browser/device permissions and one historical item query. Only then record a successful recovery test.

For an actual outage, stop ingestion and notification workers, restore to a new database, validate it, update the service database configuration, and restart the service. Keep the previous database until the recovery is accepted. Do not replay old alerts as new opportunities. The default compose file names the database `nauctions`; a cutover to a restored name requires an explicit reviewed configuration change.

## Release and rollback

CI checks Rust formatting, Clippy, workspace tests, frontend tests/build, and dependency audits. It builds and uploads a Windows collector ZIP. The collector package includes usage instructions; its build script prints SHA-256 hashes. Review all CI results before release. Actions and external images need a supply-chain review before production use.

Before a release, take and verify an encrypted backup. Record the current image digest and schema version. Deploy the tested revision and verify health, authorization, upload receipt transitions, history, and notification suppression. On failure, stop the new service. Use the previous image only if it supports the current schema. If not, follow the restore procedure into a new database. Do not automatically run down migrations against production data.

## Local validation record

On 27 October 2026, the local Docker Linux backend was started and the PostgreSQL 16 service became healthy on 127.0.0.1:55439. The earlier 55432 port conflicted with an unrelated application and was not changed. Both Compose configurations passed validation. The Caddy configuration passed validation in its official container. The final production image built successfully with Node 22, the security-updated npm lockfile, and the PostgreSQL-only storage facade and embedded migrations. Its npm install reported no vulnerabilities. A temporary loopback container passed health and static-frontend checks as UID 10001 with a read-only root filesystem. Startup applied migration 2 to the disposable restored database and preserved its 1 member and 3 scans. The test container was then stopped.

A released Windows collector paired with the local test API and uploaded the synthetic fixture. Its source-file checksum stayed unchanged. The server reported acceptance before the collector cleared the spool bytes. A local age-encrypted backup restored into the new database `restore_populated_20261027`. Counts matched: 1 migration, 1 member, 1 device, 1 upload, and 3 scans. No unvalidated foreign keys were present. A separate API process on loopback port 18081 accepted replay of the original upload UUID. Counts remained 1 member, 1 upload, 3 scans, and 0 alerts. The temporary API process was stopped. Plaintext dump files were removed by the scripts.

This drill used `-LocalOnly` and a temporary test age identity. It did not test offsite storage or a real game file. Production deployment, offsite credentials, remote backup verification, and real-client validation remain pending. The local test identity and generated credentials are in ignored `target/` and `data/` paths; they are not production recovery keys.






The final local image after the dependency cleanup is `nauctions-local-build-check`. Its manifest digest is `sha256:74cee4743a885d55c5eac33a4b604d02202aa2de24525e3a7c069c9e02b0a60b`. A repeat smoke test migrated a separate restored database from version 1 to 2 and accepted the original upload replay. Final counts were 2 migrations, 1 member, 1 upload, 3 scans, and 0 alerts. The temporary container was stopped. This image has not been pushed to a registry.


For an existing production volume, the initialization script does not run again. Do not delete a populated volume to enable the new role. Stop the backend and have the database administrator create the role, transfer ownership of application tables and the public schema, and update the backend connection. Back up and validate that migration before restart. A restore performed by the maintenance role can create maintenance-owned objects; transfer the restored application objects to `market_app` and verify migrations under that role before a production cutover. Local development keeps its existing disposable database user.


The final image also passed a fresh isolated PostgreSQL initialization test under `market_app`. The role has no superuser, database-creation, role-creation, replication, or row-security-bypass privileges. A password containing a quote, backslash, and dollar sign authenticated successfully. Equal maintenance/application passwords were refused before initialization. The application ran both migrations, created its bootstrap member, owned every application table, and served health and static frontend requests. Database CREATE and TEMPORARY privileges were absent. Only the new test containers, volume, and network were removed; existing data was not changed.

