#!/usr/bin/env python3
"""Query OSV for locked crates.io dependencies; explicitly uses the network.

Only package names and versions leave this machine, never Cargo metadata or
local paths. Git dependencies are reported as outside this registry audit.
API contract: https://google.github.io/osv.dev/post-v1-querybatch/
Run separately from the offline tools/check.sh gate.
"""

import json
from pathlib import Path
import re
import subprocess
import sys
from urllib import error, request


API = "https://api.osv.dev/v1/querybatch"
REGISTRIES = {
    "registry+https://github.com/rust-lang/crates.io-index",
    "sparse+https://index.crates.io/",
}
BATCH_SIZE = 100
MAX_PAGES = 100
MAX_RESPONSE_BYTES = 8 * 1024 * 1024
# Maintenance warning, not a blanket exemption for this crate's vulnerabilities.
ALLOWED_WARNING = ("paste", "RUSTSEC-2024-0436")


class AuditError(Exception):
    """A lookup failed or cannot be interpreted safely."""


def registry_packages(metadata):
    """Return unique package/version pairs and visible coverage exclusions."""
    if not isinstance(metadata, dict) or not isinstance(metadata.get("packages"), list):
        raise AuditError("Cargo metadata has no package list")
    packages, unsupported = set(), set()
    for package in metadata["packages"]:
        if not isinstance(package, dict):
            raise AuditError("Cargo metadata contains an invalid package")
        name, version, source = (package.get(key) for key in ("name", "version", "source"))
        if not all(isinstance(value, str) and value for value in (name, version)):
            raise AuditError("Cargo metadata contains an unnamed or unversioned package")
        if source is None:
            continue  # Workspace/path code is covered by repository review, not OSV names.
        if not isinstance(source, str):
            raise AuditError("Cargo metadata contains an invalid package source")
        if source in REGISTRIES:
            packages.add((name, version))
        else:
            kind = "Git" if source.startswith("git+") else "non-crates.io"
            unsupported.add(f"{name} {version}: {kind} dependency is NOT audited")
    if not packages:
        raise AuditError("Cargo metadata contains no crates.io dependencies to audit")
    return sorted(packages), sorted(unsupported)


def query_for(package, page_token=None):
    name, version = package
    query = {"package": {"ecosystem": "crates.io", "name": name}, "version": version}
    if page_token:
        query["page_token"] = page_token
    return query


def post_json(payload):
    encoded = json.dumps(payload).encode("utf-8")
    req = request.Request(
        API, data=encoded, method="POST",
        headers={"Content-Type": "application/json", "User-Agent": "Aede-dependency-audit"},
    )
    try:
        with request.urlopen(req, timeout=30) as response:
            if response.status != 200:
                raise AuditError(f"OSV returned HTTP {response.status}")
            body = response.read(MAX_RESPONSE_BYTES + 1)
        if len(body) > MAX_RESPONSE_BYTES:
            raise AuditError("OSV response exceeds the audit size limit")
        return json.loads(body)
    except (error.URLError, OSError, ValueError) as failure:
        raise AuditError(f"OSV lookup failed: {failure}") from failure


def lookup(packages, post=post_json):
    findings = {package: set() for package in packages}
    for start in range(0, len(packages), BATCH_SIZE):
        pending = [(package, None) for package in packages[start:start + BATCH_SIZE]]
        seen_tokens = {package: set() for package, _ in pending}
        for _ in range(MAX_PAGES):
            response = post({"queries": [query_for(*item) for item in pending]})
            results = response.get("results") if isinstance(response, dict) and set(response) == {"results"} else None
            if not isinstance(results, list) or len(results) != len(pending):
                raise AuditError("OSV returned a missing or mismatched result list")
            next_page = []
            for (package, _), result in zip(pending, results):
                if not isinstance(result, dict) or not set(result) <= {"vulns", "next_page_token"}:
                    raise AuditError("OSV returned an unrecognized result")
                vulns = result.get("vulns", [])
                if not isinstance(vulns, list):
                    raise AuditError("OSV returned an invalid advisory list")
                for advisory in vulns:
                    identifier = advisory.get("id") if isinstance(advisory, dict) else None
                    if not isinstance(identifier, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._:-]*", identifier):
                        raise AuditError("OSV returned an advisory without a valid ID")
                    findings[package].add(identifier)
                token = result.get("next_page_token", "")
                if not isinstance(token, str):
                    raise AuditError("OSV returned an invalid pagination token")
                if token:
                    if token in seen_tokens[package]:
                        raise AuditError("OSV repeated a pagination token")
                    seen_tokens[package].add(token)
                    next_page.append((package, token))
            if not next_page:
                break
            pending = next_page
        else:
            raise AuditError("OSV pagination exceeded the audit page limit")
    return findings


def classify(findings):
    warnings, failures = [], []
    for (name, version), identifiers in sorted(findings.items()):
        for identifier in sorted(identifiers):
            detail = f"{name} {version}: {identifier} https://osv.dev/vulnerability/{identifier}"
            if (name, identifier) == ALLOWED_WARNING:
                warnings.append(f"{detail} (accepted maintenance warning: paste is unmaintained)")
            else:
                failures.append(detail)
    return warnings, failures


def main():
    try:
        completed = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--offline", "--all-features"],
            cwd=Path(__file__).resolve().parent.parent,
            check=True, capture_output=True, text=True, timeout=120,
        )
        packages, unsupported = registry_packages(json.loads(completed.stdout))
        for excluded in unsupported:
            print(f"WARNING: {excluded}; review its pinned source separately.", flush=True)
        print(f"Querying OSV for {len(packages)} locked crates.io package versions (names and versions only).", flush=True)
        warnings, failures = classify(lookup(packages))
        for warning in warnings:
            print(f"WARNING: {warning}")
        for failure in failures:
            print(f"ERROR: {failure}")
        if failures:
            return 1
        print("Registry audit passed; coverage exclusions and accepted warnings above still apply.")
        return 0
    except (AuditError, OSError, ValueError, subprocess.SubprocessError) as failure:
        print(f"ERROR: dependency audit incomplete: {failure}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
