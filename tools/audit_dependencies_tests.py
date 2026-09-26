"""Offline behavioral tests for the explicit OSV dependency audit."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock, patch
from urllib import error


spec = importlib.util.spec_from_file_location(
    "audit_dependencies", Path(__file__).with_name("audit-dependencies.py")
)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def package(name="paste", version="1.0.15", source=REGISTRY):
    return {"name": name, "version": version, "source": source}


def result(*identifiers, token=None):
    value = {"vulns": [{"id": identifier} for identifier in identifiers]}
    if token is not None:
        value["next_page_token"] = token
    return value


class RegistrySelectionTests(unittest.TestCase):
    def test_registry_pairs_are_deduplicated_and_non_registry_sources_are_visible(self):
        packages, excluded = audit.registry_packages({"packages": [
            package(), package(), package("aede", source=None),
            package("private-git", source="git+https://private.invalid/repository#abc"),
            package("private-registry", source="registry+https://private.invalid/index"),
        ]})
        self.assertEqual(packages, [("paste", "1.0.15")])
        self.assertTrue(any("private-git" in value and "Git" in value for value in excluded))
        self.assertTrue(any("private-registry" in value for value in excluded))
        self.assertTrue(all("NOT audited" in value for value in excluded))
        query = audit.query_for(packages[0])
        self.assertEqual(query, {
            "package": {"ecosystem": "crates.io", "name": "paste"}, "version": "1.0.15"
        })
        self.assertNotIn("private", json.dumps(query))

    def test_empty_or_malformed_metadata_is_not_a_clean_audit(self):
        for metadata in [{}, [], {"packages": []}, {"packages": [None]},
                         {"packages": [package(source={})]}, {"packages": [package(name="")]}]:
            with self.subTest(metadata=metadata), self.assertRaises(audit.AuditError):
                audit.registry_packages(metadata)


class QueryTests(unittest.TestCase):
    def test_pagination_keeps_package_order_and_follows_only_unfinished_queries(self):
        packages = [("first", "1.0"), ("second", "2.0")]
        post = Mock(side_effect=[
            {"results": [result("RUSTSEC-1", token="page-2"), {}]},
            {"results": [{"next_page_token": "page-3"}]},
            {"results": [result("RUSTSEC-1", "RUSTSEC-2")]},
        ])
        found = audit.lookup(packages, post)
        self.assertEqual(found, {packages[0]: {"RUSTSEC-1", "RUSTSEC-2"}, packages[1]: set()})
        self.assertEqual(post.call_args_list[0].args[0], {
            "queries": [audit.query_for(item) for item in packages]
        })
        for index, token in [(1, "page-2"), (2, "page-3")]:
            self.assertEqual(post.call_args_list[index].args[0], {
                "queries": [audit.query_for(packages[0], token)]
            })

    def test_large_package_lists_are_batched_without_losing_a_result(self):
        packages = [(str(number), "1.0") for number in range(3)]
        post = Mock(side_effect=[{"results": [{}, {}]}, {"results": [result("RUSTSEC-3")]}])
        with patch.object(audit, "BATCH_SIZE", 2):
            found = audit.lookup(packages, post)
        self.assertEqual(found[packages[2]], {"RUSTSEC-3"})
        self.assertEqual(len(post.call_args_list[0].args[0]["queries"]), 2)
        self.assertEqual(len(post.call_args_list[1].args[0]["queries"]), 1)

    def test_incomplete_or_error_results_fail_closed(self):
        for response in [
            {}, {"error": "unavailable"}, {"results": []},
            {"results": [{}], "error": "partial"}, {"results": [None]},
            {"results": [{"error": "unavailable"}]},
            {"results": [{"vulns": None}]}, {"results": [{"vulns": [{}]}]},
            {"results": [result("bad\nidentifier")]},
            {"results": [result(token=17)]},
        ]:
            with self.subTest(response=response), self.assertRaises(audit.AuditError):
                audit.lookup([("paste", "1.0")], Mock(return_value=response))

    def test_repeated_tokens_and_unbounded_pagination_are_refused(self):
        repeated = Mock(return_value={"results": [result(token="repeat")]})
        with self.assertRaisesRegex(audit.AuditError, "repeated"):
            audit.lookup([("paste", "1.0")], repeated)
        with patch.object(audit, "MAX_PAGES", 1):
            with self.assertRaisesRegex(audit.AuditError, "page limit"):
                audit.lookup([("paste", "1.0")], repeated)

    def test_http_errors_and_invalid_json_never_become_empty_findings(self):
        for failure in [error.URLError("offline"), TimeoutError("timeout")]:
            with patch.object(audit.request, "urlopen", side_effect=failure):
                with self.assertRaises(audit.AuditError):
                    audit.post_json({"queries": []})
        for body, status in [(b"not json", 200), (b"{}", 503)]:
            response = Mock(status=status)
            response.read.return_value = body
            response.__enter__ = Mock(return_value=response)
            response.__exit__ = Mock(return_value=False)
            with patch.object(audit.request, "urlopen", return_value=response):
                with self.assertRaises(audit.AuditError):
                    audit.post_json({"queries": []})

    def test_http_request_uses_only_the_explicit_osv_payload_and_a_timeout(self):
        response = Mock(status=200)
        response.read.return_value = b'{"results":[{}]}'
        response.__enter__ = Mock(return_value=response)
        response.__exit__ = Mock(return_value=False)
        payload = {"queries": [audit.query_for(("paste", "1.0.15"))]}
        with patch.object(audit.request, "urlopen", return_value=response) as opened:
            self.assertEqual(audit.post_json(payload), {"results": [{}]})
        req = opened.call_args.args[0]
        self.assertEqual(req.full_url, audit.API)
        self.assertEqual(req.method, "POST")
        self.assertEqual(json.loads(req.data), payload)
        self.assertEqual(opened.call_args.kwargs["timeout"], 30)

    def test_oversized_response_is_refused_before_json_parsing(self):
        response = Mock(status=200)
        response.read.return_value = b"too large"
        response.__enter__ = Mock(return_value=response)
        response.__exit__ = Mock(return_value=False)
        with patch.object(audit, "MAX_RESPONSE_BYTES", 4):
            with patch.object(audit.request, "urlopen", return_value=response):
                with self.assertRaisesRegex(audit.AuditError, "size limit"):
                    audit.post_json({"queries": []})
        response.read.assert_called_once_with(5)


class PolicyTests(unittest.TestCase):
    def test_only_the_exact_paste_maintenance_advisory_is_a_visible_warning(self):
        warnings, failures = audit.classify({
            ("paste", "1.0.15"): {"RUSTSEC-2024-0436", "RUSTSEC-new"},
            ("different-crate", "1.0"): {"RUSTSEC-2024-0436"},
        })
        self.assertEqual(len(warnings), 1)
        self.assertIn("unmaintained", warnings[0])
        self.assertEqual(len(failures), 2)

    def test_command_fails_on_metadata_errors_lookup_errors_and_new_advisories(self):
        metadata = subprocess.CompletedProcess([], 0, json.dumps({"packages": [package()]}))
        cases = [
            (audit.AuditError("offline"), 2),
            ({("paste", "1.0.15"): {"RUSTSEC-new"}}, 1),
            ({("paste", "1.0.15"): {"RUSTSEC-2024-0436"}}, 0),
        ]
        for outcome, expected in cases:
            lookup = Mock(side_effect=outcome) if isinstance(outcome, Exception) else Mock(return_value=outcome)
            with patch.object(audit.subprocess, "run", return_value=metadata) as run:
                with patch.object(audit, "lookup", lookup), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(audit.main(), expected)
            command = run.call_args.args[0]
            self.assertIn("--locked", command)
            self.assertIn("--offline", command)
            self.assertIn("--all-features", command)
        with patch.object(audit.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "cargo")):
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(audit.main(), 2)


if __name__ == "__main__":
    unittest.main()
