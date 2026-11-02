"""Run on the Coolify host. Credentials never leave that host.

Provision and deploy only the separate Nauctions project. Read the repository's
Coolify skill before use. The token needs read/write/deploy, not read:sensitive.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import urllib.error
import urllib.request

from coolify_guard import SERVER_UUID, ScopeError, check_mutation, check_origin, check_resource

STATE = Path("/root/nauctions-deployment.json")
SECRETS = Path("/root/nauctions-runtime.json")
TOKEN = Path("/root/nauctions-api-token")
BASELINE = Path("/var/lib/nauctions-deploy/protected-before.json")


def save(path, value):
    temporary = path.with_suffix(".partial")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as stream:
        json.dump(value, stream, indent=2)
    os.replace(temporary, path)


class Deployment:
    def __init__(self):
        self.state = json.loads(STATE.read_text()) if STATE.exists() else {"server_uuid": SERVER_UUID}
        self.token = None

    def api(self, path, payload=None, method="GET"):
        if method != "GET":
            check_mutation(self.state, method, path, payload)
        if self.token is None:
            self.token = TOKEN.read_text().strip()
        request = urllib.request.Request("http://127.0.0.1:8000/api/v1/" + path,
            data=None if payload is None else json.dumps(payload).encode(), method=method,
            headers={"Authorization": "Bearer " + self.token, "Content-Type": "application/json", "Accept": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            # Validation response field names are safe. Values and raw bodies are not.
            try:
                fields = sorted(json.loads(error.read()).get("errors", {}).keys())
            except (ValueError, AttributeError):
                fields = []
            raise RuntimeError(f"Coolify request failed: HTTP {error.code}; invalid fields: {fields}") from None

    def verify_protected(self):
        baseline = json.loads(BASELINE.read_text())
        for old in baseline["containers"]:
            inspected = subprocess.check_output(["docker", "inspect", old["id"]])
            current = json.loads(inspected)[0]
            selected = {key: current[key] for key in ["Id", "Name", "Image", "Config", "HostConfig", "Mounts"]}
            digest = hashlib.sha256(json.dumps(selected, sort_keys=True).encode()).hexdigest()
            if digest != old["config_sha256"] or current["State"]["StartedAt"] != old["started_at"]:
                raise ScopeError("A protected container changed; stop and inspect without redeploying it")
        return len(baseline["containers"])

    def verify_owner(self, kind):
        resource = self.api(f"{kind}s/{self.state[kind + '_uuid']}")
        check_resource(self.state, kind, resource)
        return resource

    def provision(self):
        self.verify_protected()
        if not self.state.get("project_uuid"):
            if any(p["name"] == "Nauctions" for p in self.api("projects")):
                raise ScopeError("An unrecorded Nauctions project exists; inspect it before adoption")
            project = self.api("projects", {"name": "Nauctions", "description": "Separate shared auction-history service"}, "POST")
            self.state["project_uuid"] = project["uuid"]
            save(STATE, self.state)
        project = self.api("projects/" + self.state["project_uuid"])
        if project["name"] != "Nauctions":
            raise ScopeError("Recorded project name differs")
        environments = self.api("projects/" + self.state["project_uuid"] + "/environments")
        if isinstance(environments, dict):
            environments = environments.get("environments", environments.get("data", []))
        production = [env for env in environments if env["name"] == "production"]
        if len(production) != 1:
            raise ScopeError("Expected the project's default production environment")
        self.state["environment_id"] = production[0]["id"]
        self.state["environment_uuid"] = production[0]["uuid"]
        save(STATE, self.state)
        if not SECRETS.exists():
            save(SECRETS, {"postgres_password": secrets.token_hex(32), "app_db_password": secrets.token_hex(32),
                "staging_key_hex": secrets.token_hex(32), "admin_login": "admin", "admin_password": secrets.token_urlsafe(32)})
        credentials = json.loads(SECRETS.read_text())
        if not self.state.get("database_uuid"):
            database = self.api("databases/postgresql", {"name": "nauctions-postgres", "server_uuid": SERVER_UUID,
                "project_uuid": self.state["project_uuid"], "environment_name": "production",
                "postgres_user": "nauctions_admin", "postgres_password": credentials["postgres_password"],
                "postgres_db": "nauctions", "image": "postgres:16-bookworm", "is_public": False,
                "instant_deploy": False, "limits_memory": "1g", "limits_cpus": "1"}, "POST")
            self.state["database_uuid"] = database["uuid"]
            save(STATE, self.state)
        database = self.verify_owner("database")
        if database.get("is_public"):
            raise ScopeError("Nauctions database has a public port")
        print(json.dumps({"project_uuid": self.state["project_uuid"], "database_uuid": self.state["database_uuid"],
                          "environment_id": self.state["environment_id"]}))

    def start_database(self):
        self.verify_owner("database")
        self.api(f"databases/{self.state['database_uuid']}/start", method="POST")
        print("Nauctions database start requested")

    def initialize_database(self):
        self.verify_protected()
        self.verify_owner("database")
        uuid = self.state["database_uuid"]
        credentials = json.loads(SECRETS.read_text())
        base = ["docker", "exec", "-i", uuid, "psql", "-X", "-v", "ON_ERROR_STOP=1", "-U", "nauctions_admin", "-d", "nauctions"]
        exists = subprocess.run(base + ["-tAc", "SELECT count(*) FROM pg_roles WHERE rolname='market_app'"], capture_output=True, text=True, check=True)
        if exists.stdout.strip() != "0":
            raise ScopeError("Application role already exists; verify its credentials and grants before changing it")
        sql = """CREATE ROLE market_app WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD :'app_password';
REVOKE ALL ON DATABASE nauctions FROM PUBLIC;
GRANT CONNECT ON DATABASE nauctions TO market_app;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
ALTER SCHEMA public OWNER TO market_app;
GRANT USAGE, CREATE ON SCHEMA public TO market_app;
"""
        result = subprocess.run(base + ["--set=app_password=" + credentials["app_db_password"]], input=sql, capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError("Nauctions database role initialization failed; no raw SQL output shown")
        print("Created the restricted Nauctions application role")

    def application(self, origin, realm, house, tag, revision):
        check_origin(origin)
        self.verify_protected()
        self.verify_owner("database")
        credentials = json.loads(SECRETS.read_text())
        for app in self.api("applications"):
            if app.get("uuid") != self.state.get("application_uuid") and origin in (app.get("fqdn") or "").split(","):
                raise ScopeError("Hostname belongs to another application")
        if not self.state.get("application_uuid"):
            app = self.api("applications/dockerimage", {"name": "nauctions-server", "server_uuid": SERVER_UUID,
                "project_uuid": self.state["project_uuid"], "environment_name": "production",
                "docker_registry_image_name": "localhost:5000/nauctions-server", "docker_registry_image_tag": tag,
                "ports_exposes": "8080", "domains": origin, "autogenerate_domain": False, "instant_deploy": False,
                "health_check_enabled": True, "health_check_path": "/healthz", "health_check_port": "8080",
                "health_check_method": "GET", "health_check_return_code": 200,
                "limits_memory": "768m", "limits_cpus": "1", "is_auto_deploy_enabled": False}, "POST")
            self.state["application_uuid"] = app["uuid"]
            save(STATE, self.state)
        self.verify_owner("application")
        self.api("applications/" + self.state["application_uuid"], {"docker_registry_image_name": "localhost:5000/nauctions-server",
            "docker_registry_image_tag": tag, "domains": origin}, "PATCH")
        environment = {
            "DATABASE_URL": "postgresql://market_app:" + credentials["app_db_password"] + "@" + self.state["database_uuid"] + ":5432/nauctions",
            "BIND_ADDR": "0.0.0.0:8080", "PUBLIC_ORIGIN": origin, "MARKET_REALM": realm, "MARKET_HOUSE": house,
            "STAGING_KEY_HEX": credentials["staging_key_hex"], "BOOTSTRAP_ADMIN_LOGIN": credentials["admin_login"],
            "BOOTSTRAP_ADMIN_PASSWORD": credentials["admin_password"], "ALLOW_INSECURE_LOCALHOST": "false",
            "TRUST_RECORDED_SCAN_COMPLETENESS": "false", "NAUCTIONS_TELEMETRY": "0", "RUST_LOG": "info"}
        existing = {item["key"] for item in self.api("applications/" + self.state["application_uuid"] + "/envs")}
        for key, value in environment.items():
            self.api("applications/" + self.state["application_uuid"] + "/envs", {"key": key, "value": value,
                "is_literal": True, "is_runtime": True, "is_buildtime": False, "is_preview": False},
                "PATCH" if key in existing else "POST")
        self.state.update(origin=origin, realm=realm, house=house, image_tag=tag, source_revision=revision)
        save(STATE, self.state)
        print(json.dumps({"application_uuid": self.state["application_uuid"], "origin": origin, "configured": True}))

    def deploy(self):
        self.verify_protected()
        self.verify_owner("application")
        self.verify_owner("database")
        result = self.api("deploy", {"uuid": self.state["application_uuid"]}, "POST")
        print(json.dumps(result))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["provision", "start-database", "initialize-database", "configure", "deploy", "verify"])
    parser.add_argument("--origin")
    parser.add_argument("--realm")
    parser.add_argument("--house", choices=["Horde", "Alliance", "Neutral"])
    parser.add_argument("--image-tag")
    parser.add_argument("--revision")
    args = parser.parse_args()
    deployment = Deployment()
    if args.action == "provision":
        deployment.provision()
    elif args.action == "start-database":
        deployment.start_database()
    elif args.action == "initialize-database":
        deployment.initialize_database()
    elif args.action == "configure":
        if not all([args.origin, args.realm, args.house, args.image_tag, args.revision]):
            parser.error("configure requires origin, realm, house, image-tag and revision")
        deployment.application(args.origin, args.realm, args.house, args.image_tag, args.revision)
    elif args.action == "deploy":
        deployment.deploy()
    else:
        count = deployment.verify_protected()
        resources = {}
        for kind in ["database", "application"]:
            if deployment.state.get(kind + "_uuid"):
                live = deployment.verify_owner(kind)
                resources[kind] = {"uuid": live["uuid"], "name": live["name"], "status": live.get("status")}
        print(json.dumps({"protected_containers_unchanged": count, "nauctions": resources}))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ScopeError) as error:
        raise SystemExit(str(error)) from None
