"""Reject Coolify mutations outside the recorded Nauctions resources.

This guard supplements team-scoped API permissions. It does not grant access.
The caller must read the current resource and verify ownership before sending
an update. Never include a token or an environment value in an error message.
"""

import re

SERVER_UUID = "jw0rnw3it1lljl1jyqzkddno"
PROTECTED_UUIDS = frozenset(
    {"pdcglnpo8ebmhm3xg2ikh46l", "uqdmlbainoxxahxb3f87xlqr", "cuxvhtoeb3vipgxtfzxxap7y"}
)


class ScopeError(ValueError):
    pass


def _values(value):
    if isinstance(value, dict):
        for item in value.values():
            yield from _values(item)
    elif isinstance(value, list):
        for item in value:
            yield from _values(item)
    else:
        yield value


def check_mutation(state, method, path, payload=None):
    """Allow only the supported Nauctions provisioning/deployment requests."""
    payload = payload or {}
    method = method.upper()
    if "?" in path or path.startswith("/") or ".." in path:
        raise ScopeError("Unexpected Coolify path")
    ids = set(path.split("/")) | {v for v in _values(payload) if isinstance(v, str)}
    if ids & PROTECTED_UUIDS:
        raise ScopeError("Protected When to Plant resource")
    for key in ("project_uuid", "application_uuid", "database_uuid"):
        if state.get(key) in PROTECTED_UUIDS:
            raise ScopeError("Protected identity in Nauctions record")
    if state.get("server_uuid", SERVER_UUID) != SERVER_UUID:
        raise ScopeError("Unexpected server")
    if method == "POST" and path == "projects":
        if state.get("project_uuid") or payload.get("name") != "Nauctions":
            raise ScopeError("Unexpected project creation")
        return

    project = state.get("project_uuid")
    if not project:
        raise ScopeError("Nauctions project is not recorded")
    creates = {"databases/postgresql": "nauctions-postgres", "applications/dockerimage": "nauctions-server"}
    if method == "POST" and path in creates:
        if payload.get("name") != creates[path]:
            raise ScopeError("Unexpected resource name")
        if payload.get("project_uuid") != project or payload.get("server_uuid") != SERVER_UUID:
            raise ScopeError("Unexpected resource owner")
        if payload.get("environment_name") != "production":
            raise ScopeError("Unexpected environment")
        if path.startswith("databases/") and payload.get("is_public") is not False:
            raise ScopeError("PostgreSQL must remain private")
        if path.startswith("applications/") and payload.get("docker_registry_image_name") != "localhost:5000/nauctions-server":
            raise ScopeError("Unexpected application image")
        return

    application = state.get("application_uuid")
    database = state.get("database_uuid")
    if application and method == "POST" and path == "deploy" and payload.get("uuid") == application:
        return
    if database and method == "POST" and path == f"databases/{database}/start":
        return
    if application and path == f"applications/{application}" and method == "PATCH":
        if "project_uuid" in payload or "server_uuid" in payload or "destination_uuid" in payload:
            raise ScopeError("Resource moves are not supported")
        return
    if application and path == f"applications/{application}/envs" and method in {"POST", "PATCH"}:
        return
    if application and path == f"applications/{application}/envs/bulk" and method == "PATCH":
        return
    raise ScopeError("Mutation is outside the Nauctions allowlist")


def check_resource(state, kind, resource):
    """Verify the live UUID, exact name and production environment owner."""
    expected = {"application": "nauctions-server", "database": "nauctions-postgres"}
    if kind not in expected:
        raise ScopeError("Unknown resource kind")
    uuid = state.get(f"{kind}_uuid")
    if not uuid or uuid in PROTECTED_UUIDS or resource.get("uuid") != uuid:
        raise ScopeError("Live resource identity differs")
    if resource.get("name") != expected[kind]:
        raise ScopeError("Live resource name differs")
    environment_id = state.get("environment_id")
    if environment_id is None or str(resource.get("environment_id")) != str(environment_id):
        raise ScopeError("Live resource environment differs")


def check_origin(origin):
    if not re.fullmatch(r"https://[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?", origin):
        raise ScopeError("Use an explicit HTTPS hostname without a path or credentials")
