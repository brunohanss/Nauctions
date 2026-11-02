"""Back up only Nauctions and verify a restore into a new disposable database.

Run as root on the shared host. The bundle contains runtime secrets. Keep it
private and encrypt any off-host copy. No Coolify API token is needed.
"""
from datetime import datetime, UTC
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import uuid

from coolify_guard import PROTECTED_UUIDS

TABLES = ["_sqlx_migrations", "members", "devices", "uploads", "scans", "items", "item_observations"]
COUNTS = "SELECT json_build_object(" + ",".join(f"'{t}',(SELECT count(*) FROM {t})" for t in TABLES) + ");"


def run(args, **kwargs):
    result = subprocess.run(args, capture_output=True, timeout=300, **kwargs)
    if result.returncode:
        raise RuntimeError("Nauctions backup command failed; raw output withheld")
    return result.stdout


def main():
    state_path = Path("/root/nauctions-deployment.json")
    state = json.loads(state_path.read_text())
    db = state["database_uuid"]
    if db in PROTECTED_UUIDS or state["project_uuid"] != "xkized560ricywkoxqun1t8i" or db != "jjx6ddptanfpadcfhsdsd79v":
        raise RuntimeError("Unexpected database ownership; no backup command sent")
    container = json.loads(run(["docker", "inspect", db]))[0]
    if container["Name"] != "/" + db or "POSTGRES_DB=nauctions" not in container["Config"]["Env"]:
        raise RuntimeError("Live database identity differs")
    root = Path("/var/backups/nauctions")
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    stamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%S%fZ")
    stem = root / ("nauctions-" + stamp)
    dump = stem.with_suffix(".dump")
    record = stem.with_suffix(".verification.json")
    bundle = stem.with_suffix(".tar.gz")
    psql = ["docker", "exec", "-i", db, "psql", "-X", "-qAt", "-v", "ON_ERROR_STOP=1", "-U", "nauctions_admin", "-d"]
    holder = subprocess.Popen(psql + ["nauctions"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        holder.stdin.write("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\nSELECT pg_export_snapshot();\n" + COUNTS + "\n")
        holder.stdin.flush()
        snapshot = holder.stdout.readline().strip()
        if not re.fullmatch(r"[0-9A-Fa-f-]+", snapshot):
            raise RuntimeError("No exported PostgreSQL snapshot")
        expected = json.loads(holder.stdout.readline())
        fd = os.open(dump, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "wb") as stream:
            result = subprocess.run(["docker", "exec", db, "pg_dump", "-U", "nauctions_admin", "-d", "nauctions",
                "--format=custom", "--no-owner", "--snapshot=" + snapshot], stdout=stream, stderr=subprocess.PIPE, timeout=300)
        if result.returncode:
            raise RuntimeError("Snapshot dump failed")
        holder.stdin.write("COMMIT;\n\\q\n")
        holder.stdin.flush()
        holder.wait(timeout=20)
    finally:
        if holder.poll() is None:
            holder.kill()
        holder.communicate()
    target = "nauctions_verify_" + uuid.uuid4().hex
    run(["docker", "exec", db, "createdb", "-U", "nauctions_admin", "--owner=market_app", target])
    # Only this just-created random database can be restored or removed.
    with dump.open("rb") as source:
        run(["docker", "exec", "-i", db, "pg_restore", "-U", "nauctions_admin", "--role=market_app", "--no-owner",
             "--no-privileges", "--exit-on-error", "--dbname=" + target], stdin=source)
    restored = json.loads(run(psql + [target, "-c", COUNTS]))
    if restored != expected:
        raise RuntimeError("Restored counts differ; retain the restore database for inspection")
    run(["docker", "exec", db, "dropdb", "-U", "nauctions_admin", target])
    with dump.open("rb") as stream:
        dump_hash = hashlib.file_digest(stream, "sha256").hexdigest()
    verification = {"created_at": datetime.now(UTC).isoformat(), "source_revision": state["source_revision"],
        "database_uuid": db, "source_database": "nauctions", "snapshot_counts": expected,
        "restored_counts": restored, "dump_sha256": dump_hash, "verification": "shared-snapshot-restore-v1"}
    fd = os.open(record, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as stream:
        json.dump(verification, stream, indent=2)
    fd = os.open(bundle, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as stream:
        with tarfile.open(fileobj=stream, mode="w:gz") as archive:
            for path, name in [(dump, "database.dump"), (record, "verification.json"),
                (Path("/root/nauctions-runtime.json"), "runtime.json"), (state_path, "deployment.json")]:
                archive.add(path, arcname=name)
    with tarfile.open(bundle) as archive:
        with archive.extractfile("database.dump") as stream:
            if hashlib.file_digest(stream, "sha256").hexdigest() != dump_hash:
                raise RuntimeError("Bundled dump differs")
    with bundle.open("rb") as stream:
        verification["bundle_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
    verification["bundle"] = str(bundle)
    record.write_text(json.dumps(verification, indent=2))
    dump.unlink()  # Only the new plaintext file; its verified bundle remains.
    print(json.dumps(verification))


if __name__ == "__main__":
    main()
