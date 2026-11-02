---
name: nauctions-coolify
description: Deploy or operate Nauctions on the shared Coolify Linux host while preserving When to Plant resources and workloads.
---

# Nauctions on the shared Coolify host

Use this skill for Coolify, SSH, production configuration, database, backup, domain, or remote deployment work in this repository.

## Resource boundary

The inspected host is `ubuntu@51.158.54.37`. Coolify listens on host loopback port 8000. Reuse the host and its existing proxy through normal Coolify resource operations. Do not reinstall Coolify or add a second proxy on ports 80 and 443.

These are protected When to Plant resources, verified on 30 October 2026:

| Resource | ID |
| --- | --- |
| Coolify project | `pdcglnpo8ebmhm3xg2ikh46l` |
| Application | `uqdmlbainoxxahxb3f87xlqr` |
| Database | `cuxvhtoeb3vipgxtfzxxap7y` |
| Shared server | `jw0rnw3it1lljl1jyqzkddno` |

The server ID is shared infrastructure, not permission to change other resources. Preserve When to Plant application settings, environment values, routes, containers, images, database, roles, volumes, backups, timers, and acquisition/recovery jobs. Do not stop or restart them, run their migration/deploy scripts, or change their source files for a Nauctions task. Editing their agent instructions requires a user request, as in the initial isolation-skill setup.

Create a separate `Nauctions` project and separate `nauctions-` application and database resources. Before a later update, read the exact resource by UUID and verify its name, project, environment and server against the Nauctions deployment record. Reject a protected ID or an unknown owner. Do not select the first application/database in a list. A matching name alone does not establish identity.

The live Nauctions project is `xkized560ricywkoxqun1t8i`, application `oie1i7qsa2ewyouekfkuulkk`, database `jjx6ddptanfpadcfhsdsd79v`, production environment ID `2`. The approved origin is `https://nauctions.when-to-plant.net`, market `Normal` / `Horde`. Read `docs/coolify-deployment.md` before changes. The host's Cloudflare-only web firewall must remain in place. The direct sslip.io address failed; do not reactivate it or open the firewall as a shortcut. The generic Python probe's Cloudflare 1010 was not a companion failure: the native client passed without a security exception.

Use `/root/nauctions-deployment.json` for the non-public resource record and `/root/nauctions-` files for credentials. Use `/var/backups/nauctions` for remote backups. Keep credentials host-only with mode 0600. Do not copy When to Plant database passwords, staging keys or application secrets. Do not overwrite its credential files. Keep local evidence under ignored `artifacts/` and exclude secrets from output and Git.

An off-host Nauctions recovery export requires explicit authorization for its contents and destination. The user approved one bundle's database/runtime-secret transfer to `D:/Dev/Nauctions/backups/coolify` on 30 October 2026. It is encrypted with Windows CurrentUser DPAPI before local storage and verified by an in-memory decrypt/hash check. That approval does not authorize recurring exports or transfers to another destination. Do not write a plaintext local recovery bundle.

## Deploy within the requested scope

1. Inspect existing resources, free capacity, and active workloads without changing them. Record protected resource/container identities before deployment.
2. Prepare Nauctions files and a source revision. Use a dedicated image name and tag. Limit build/runtime resource use. Do not prune shared images, build caches, volumes, or networks.
3. Use Coolify to create only the Nauctions resources. PostgreSQL must have no public port. Use a separate restricted application role. Set an explicit HTTPS origin and market realm/faction. Keep shared-history writes disabled and completeness untrusted.
4. Use `scripts/deploy-coolify.py` with `scripts/coolify_guard.py` for supported API operations. Run `python scripts/test-coolify-guard.py` after guard changes. The helper verifies the live name and production environment before updates and rejects protected IDs and shared-server changes. If an API call fails, inspect that failure. Do not run a broad provision/reset script as recovery.
5. Check the exact Nauctions health endpoint, authentication boundary, migrations, database-role restrictions, persistent storage, and backup/restore behavior. Compare protected settings and container identities with the before-deployment record. Report any difference; do not repair another app without authority.
6. Record the Nauctions IDs, revision, URL, image, verification results and remaining gates. Do not claim production acceptance from a container startup alone.

A skill is an instruction guard, not a technical access-control boundary. A team-scoped Coolify token can reach other team resources. Use the smallest available permissions and lifetime, and revoke a temporary deployment token when finished. Do not rotate or replace another application's token to obtain access.
