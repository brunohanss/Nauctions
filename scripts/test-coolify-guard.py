"""Exercise wrong-target failures before any live deployment request."""
import unittest

from coolify_guard import PROTECTED_UUIDS, SERVER_UUID, ScopeError, check_mutation, check_origin, check_resource


class DeploymentIsolation(unittest.TestCase):
    def setUp(self):
        self.state = {"project_uuid": "nauctionsproject", "server_uuid": SERVER_UUID, "environment_id": 99,
                      "application_uuid": "nauctionsapp", "database_uuid": "nauctionsdb"}

    def test_protected_targets_and_forged_state_are_rejected(self):
        for uuid in PROTECTED_UUIDS:
            with self.assertRaises(ScopeError):
                check_mutation(self.state, "PATCH", f"applications/{uuid}", {"name": "nauctions-server"})
            with self.assertRaises(ScopeError):
                check_mutation({**self.state, "application_uuid": uuid}, "POST", "deploy", {"uuid": uuid})

    def test_names_and_shared_server_do_not_grant_ownership(self):
        with self.assertRaises(ScopeError):
            check_mutation(self.state, "POST", "applications/dockerimage", {"name": "nauctions-server",
                "project_uuid": "anotherproject", "server_uuid": SERVER_UUID, "environment_name": "production"})
        with self.assertRaises(ScopeError):
            check_resource(self.state, "application", {"uuid": "nauctionsapp", "name": "nauctions-server", "environment_id": 100})

    def test_public_database_global_changes_and_moves_are_rejected(self):
        with self.assertRaises(ScopeError):
            check_mutation(self.state, "POST", "databases/postgresql", {"name": "nauctions-postgres",
                "project_uuid": "nauctionsproject", "server_uuid": SERVER_UUID,
                "environment_name": "production", "is_public": True})
        for method, path in [("PATCH", "servers/" + SERVER_UUID), ("DELETE", "projects/nauctionsproject"),
                             ("POST", "applications/nauctionsapp/restart"), ("POST", "deploy?uuid=nauctionsapp")]:
            with self.assertRaises(ScopeError):
                check_mutation(self.state, method, path)
        with self.assertRaises(ScopeError):
            check_mutation(self.state, "PATCH", "applications/nauctionsapp", {"server_uuid": SERVER_UUID})

    def test_supported_nauctions_deploy_and_environment_update(self):
        check_mutation(self.state, "POST", "deploy", {"uuid": "nauctionsapp"})
        check_mutation(self.state, "POST", "applications/nauctionsapp/envs", {"key": "PUBLIC_ORIGIN", "value": "https://example.test"})
        check_resource(self.state, "database", {"uuid": "nauctionsdb", "name": "nauctions-postgres", "environment_id": 99})

    def test_https_origin_has_no_credential_or_path(self):
        check_origin("https://nauctions.example.test")
        for origin in ["http://example.test", "https://user:secret@example.test", "https://example.test/path", "https://example.test?token=x"]:
            with self.assertRaises(ScopeError):
                check_origin(origin)


if __name__ == "__main__":
    unittest.main()
