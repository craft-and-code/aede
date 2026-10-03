#!/usr/bin/env python3
"""Measure authored project sources, Git history and optional active Rust tests.

Python 3.9+, standard library only. Source measurement never invokes Cargo.
``--tests`` builds the workspace's library/binary test harnesses offline, then
lists their registered tests and ignored tests without executing either suite.
Integration tests, examples, doctests and Python helper tests are not Rust TUs.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Optional


ROOT = Path(__file__).resolve().parent.parent
LANGUAGES = {
    ".rs": "Rust", ".py": "Python", ".sh": "Shell", ".bash": "Shell",
    ".js": "JavaScript", ".ts": "TypeScript", ".tsx": "TypeScript",
    ".jsx": "JavaScript", ".css": "CSS", ".html": "HTML", ".sql": "SQL",
    ".c": "C", ".h": "C", ".cpp": "C++", ".hpp": "C++",
}
SOURCE_ROOTS = ("crates", "tools", "site")
EXCLUDED_DIRECTORIES = {
    "target", "dist", "dist-site", "node_modules", "vendor", "generated",
    "__pycache__",
}
LINE_DEFINITION = "Physical source lines, including blank lines and comments; not semantic statements."
EXCLUSIONS = [
    "Only authored source extensions under crates/, tools/ and site/ are measured.",
    "Hidden directories, symlinks, target/, dist/, dist-site/, node_modules/, vendor/, generated/ and __pycache__/ are excluded.",
    "Cargo manifests/lockfiles, documentation, data/JSON fixtures, media and other binary assets are not code lines.",
    "Rust test/support files are classified by sibling-test/fixture conventions and explicit cfg(test) path declarations; examples are separate.",
]


class StatsError(Exception):
    """The report cannot be measured or its claimed test inventory verified."""


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def files_under(directory: Path):
    """Walk deterministically, without following links or generated directories."""
    if not directory.exists() or directory.is_symlink():
        return
    for current, directories, names in os.walk(directory, followlinks=False):
        directories[:] = sorted(
            name for name in directories
            if not name.startswith(".") and name not in EXCLUDED_DIRECTORIES
            and not (Path(current) / name).is_symlink()
        )
        for name in sorted(names):
            path = Path(current) / name
            if not name.startswith(".") and not path.is_symlink() and path.is_file():
                yield path


def read_bytes(path: Path) -> bytes:
    try:
        return path.read_bytes()
    except OSError as error:
        raise StatsError(f"Cannot read {path}: {error}") from error


def source_text(data: bytes, path: Path) -> str:
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise StatsError(f"Source is not UTF-8: {path}") from error
    if "\0" in text:
        raise StatsError(f"Source contains binary NUL bytes: {path}")
    return text


def line_counts(data: bytes, path: Path) -> dict:
    text = source_text(data, path)
    lines = text.split("\n") if text else []
    if lines and lines[-1] == "":
        lines.pop()
    return {"files": 1, "physical_lines": len(lines), "nonblank_lines": sum(bool(line.strip()) for line in lines)}


def empty_count() -> dict:
    return {"files": 0, "physical_lines": 0, "nonblank_lines": 0}


def add_count(total: dict, count: dict):
    for name in total:
        total[name] += count[name]


def crate_directories(root: Path) -> dict:
    """The repository convention is one authored crate per crates/* directory."""
    crates = {}
    directory = root / "crates"
    if not directory.is_dir() or directory.is_symlink():
        return crates
    for path in sorted(directory.iterdir()):
        manifest = path / "Cargo.toml"
        if not path.is_dir() or path.is_symlink() or not manifest.is_file() or manifest.is_symlink():
            continue
        source = source_text(read_bytes(manifest), manifest)
        package = re.search(r"(?ms)^\[package\]\s*\n(.*?)(?=^\[|\Z)", source)
        name = re.search(r'^name\s*=\s*"([^"]+)"\s*(?:#.*)?$', package[1], re.M) if package else None
        if not name:
            raise StatsError(f"Expected a literal [package] name in {manifest}")
        if name[1] in crates:
            raise StatsError(f"Duplicate crate name: {name[1]}")
        crates[name[1]] = path
    return crates


def test_only_paths(sources: dict) -> set:
    """Include specially named helpers selected by explicit cfg(test) modules."""
    paths = set()
    declaration = re.compile(
        r'#\[cfg\(test\)\]\s*#\[path\s*=\s*"([^"\n]+)"\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*;'
    )
    for path, data in sources.items():
        if path.suffix != ".rs":
            continue
        for match in declaration.finditer(source_text(data, path)):
            paths.add((path.parent / match[1]).resolve())
    return paths


def category(path: Path, root: Path, test_paths: set) -> str:
    relative = path.relative_to(root)
    stem = path.stem
    if (
        "tests" in relative.parts or "testdata" in relative.parts
        or stem.endswith("_tests") or stem.endswith("_test_support")
        or stem in {"test_support", "test_fixtures"} or stem.endswith("_fixtures")
        or path.resolve() in test_paths
    ):
        return "tests"
    if "examples" in relative.parts:
        return "examples"
    if relative.parts[0] == "tools":
        return "tools"
    if relative.parts[0] == "site":
        return "website"
    return "production"


def fingerprint(root: Path, sources: dict) -> str:
    digest = hashlib.sha256()
    for path, content in sorted(sources.items(), key=lambda item: item[0].relative_to(root).as_posix()):
        name = path.relative_to(root).as_posix().encode("utf-8")
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(name)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def rust_fingerprint(root: Path) -> str:
    sources = {}
    for path in files_under(root / "crates"):
        if path.suffix == ".rs" or path.name == "Cargo.toml":
            sources[path] = read_bytes(path)
    for name in ("Cargo.toml", "Cargo.lock", ".cargo/config", ".cargo/config.toml", "rust-toolchain", "rust-toolchain.toml"):
        path = root / name
        if path.is_file() and not path.is_symlink():
            sources[path] = read_bytes(path)
    return fingerprint(root, sources)


def git_output(root: Path, *arguments: str) -> Optional[str]:
    """Read local Git metadata, without fetching or requiring Git to be present."""
    try:
        result = subprocess.run(
            ["git", *arguments], cwd=root,
            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    return result.stdout.strip() if result.returncode == 0 else None


def git_revision(root: Path) -> Optional[str]:
    return git_output(root, "rev-parse", "--verify", "HEAD^{commit}")


def git_history(root: Path) -> dict:
    """Count commits reachable from one captured HEAD, refusing shallow totals."""
    toplevel = git_output(root, "rev-parse", "--show-toplevel")
    # Git searches enclosing directories. An exported project inside another
    # checkout must not acquire that unrelated repository's history or revision.
    revision = git_revision(root) if toplevel and Path(toplevel).resolve() == root.resolve() else None
    result = {
        "status": "unavailable", "reachable_from_head": None, "lower_bound": None,
        "display": {"en": "Unavailable (Git history missing)", "fr": "Indisponible (historique Git absent)"},
        "revision": revision, "reason": "git_history_unavailable",
    }
    if revision is None:
        return result
    shallow = git_output(root, "rev-parse", "--is-shallow-repository")
    # Anchoring this command to the captured revision keeps both figures and
    # provenance consistent even if another process changes HEAD meanwhile.
    count = git_output(root, "rev-list", "--count", revision)
    if shallow not in {"true", "false"} or not count or not re.fullmatch(r"[0-9]+", count) or int(count) < 1:
        result["display"] = {"en": "Unavailable (Git history unreadable)", "fr": "Indisponible (historique Git illisible)"}
        return result
    result["reachable_from_head"] = int(count)
    if shallow == "true":
        result.update({
            "status": "partial", "reason": "shallow_history",
            "display": {"en": "Unavailable (shallow history)", "fr": "Indisponible (historique partiel)"},
        })
    else:
        bound = lower_bound(result["reachable_from_head"])
        result.update({
            "status": "complete", "reason": None, "lower_bound": bound,
            "display": {"en": f"> {bound:,} commits", "fr": "+ de " + f"{bound:,}".replace(",", " ") + " commits"},
        })
    return result


def documentation_counts(root: Path) -> dict:
    markdown = sum(path.suffix == ".md" for path in files_under(root / "docs"))
    # The publisher includes topic references that have no manifest entry. Reuse
    # its catalog so the public page count cannot drift from the actual website.
    publisher_path = Path(__file__).resolve().parent / "build-site.py"
    publisher = next((module for module in list(sys.modules.values())
                      if getattr(module, "__file__", None) == str(publisher_path)
                      and hasattr(module, "load_pages")), None)
    if publisher is None:
        name = "aede_stats_site_catalog"
        spec = importlib.util.spec_from_file_location(name, publisher_path)
        publisher = importlib.util.module_from_spec(spec)
        sys.modules[name] = publisher
        spec.loader.exec_module(publisher)
    try:
        pages = publisher.load_pages(root)
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise StatsError(f"Cannot load the published documentation catalog: {error}") from error
    return {"markdown_files": markdown, "published_pages_by_language": {
        language: sum(language in page["source"] for page in pages) for language in ("en", "fr")
    }}


def collect(root: Path = ROOT) -> dict:
    """Measure sources and local Git; unit_tests stays null without an inventory."""
    root = root.resolve()
    sources = {
        path: read_bytes(path)
        for name in SOURCE_ROOTS for path in files_under(root / name)
        if path.suffix in LANGUAGES
    }
    test_paths = test_only_paths(sources)
    crates = crate_directories(root)
    crate_rows = {
        name: {"name": name, "path": path.relative_to(root).as_posix(), "total": empty_count(),
               "production": empty_count(), "tests": empty_count(), "examples": empty_count()}
        for name, path in crates.items()
    }
    total, by_language = empty_count(), {}
    by_category = {name: empty_count() for name in ("production", "tests", "examples", "tools", "website")}
    for path, content in sources.items():
        count = line_counts(content, path)
        kind = category(path, root, test_paths)
        add_count(total, count)
        add_count(by_language.setdefault(LANGUAGES[path.suffix], empty_count()), count)
        add_count(by_category[kind], count)
        for name, directory in crates.items():
            if directory in path.parents:
                add_count(crate_rows[name]["total"], count)
                add_count(crate_rows[name][kind], count)
                break
    commits = git_history(root)
    return {
        "schema_version": 1,
        "source": {
            "scope": list(SOURCE_ROOTS), "line_definition": LINE_DEFINITION,
            "exclusions": EXCLUSIONS, "total": total,
            "by_language": dict(sorted(by_language.items())), "by_category": by_category,
            "crates": list(crate_rows.values()), "documentation": documentation_counts(root),
        },
        "commits": commits,
        "unit_tests": None,
        "provenance": {"source_fingerprint": fingerprint(root, sources), "generated_at": utc_now(), "revision": commits["revision"]},
    }


def run(command: list, root: Path, *, progress: bool = False) -> str:
    try:
        result = subprocess.run(
            command, cwd=root, stdout=subprocess.PIPE,
            stderr=None if progress else subprocess.PIPE, text=True,
            env={**os.environ, "CARGO_INCREMENTAL": "0"},
        )
    except OSError as error:
        raise StatsError(f"Cannot run {command[0]}: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.strip() if result.stderr else "see the command output"
        raise StatsError(f"Command failed ({result.returncode}): {' '.join(command)}: {detail}")
    return result.stdout


def unit_executables(messages: str, crates: dict) -> list:
    """Select Cargo's actual workspace lib/bin test artifacts, never stale target files."""
    executables = {}
    for line in messages.splitlines():
        try:
            item = json.loads(line)
        except ValueError as error:
            raise StatsError("Cargo emitted invalid JSON while building test inventories") from error
        if not isinstance(item, dict):
            raise StatsError("Cargo emitted a non-object artifact")
        if item.get("reason") != "compiler-artifact" or not item.get("profile", {}).get("test"):
            continue
        target = item.get("target", {})
        if not set(target.get("kind", [])) & {"lib", "bin"} or not isinstance(item.get("executable"), str):
            continue
        source = target.get("src_path")
        if not isinstance(source, str):
            raise StatsError("Cargo test artifact has no source path")
        source_path = Path(source).resolve()
        owner = next((name for name, directory in crates.items() if directory in source_path.parents), None)
        if owner is not None:
            executable = str(Path(item["executable"]).resolve())
            previous = executables.get(executable)
            entry = {"crate": owner, "target": target.get("name"), "executable": executable}
            if previous is not None and previous != entry:
                raise StatsError("Cargo reported an ambiguous unit-test executable")
            executables[executable] = entry
    if not executables:
        raise StatsError("Cargo returned no workspace library/binary unit-test executables")
    return sorted(executables.values(), key=lambda item: (item["crate"], item["target"]))


def listed_tests(output: str) -> set:
    tests = set()
    for line in output.splitlines():
        if line.endswith(": test"):
            name = line[:-6]
            if not name or name in tests:
                raise StatsError("Test harness listed an empty or duplicate test name")
            tests.add(name)
    summary = re.search(r"(?m)^(\d+) tests?, \d+ benchmarks?$", output)
    if summary is None or int(summary[1]) != len(tests):
        raise StatsError("Test harness output is incomplete or not standard Rust libtest output")
    return tests


def lower_bound(active: int) -> Optional[int]:
    """A strict rounded lower bound: 1500 must not be advertised as >1500."""
    if isinstance(active, bool) or not isinstance(active, int) or active < 0:
        raise StatsError("Count must be a nonnegative integer")
    if active == 0:
        return None
    below = active - 1
    step = 100 if below >= 100 else 10 if below >= 10 else 1
    return (below // step) * step


def public_labels(active: int) -> dict:
    bound = lower_bound(active)
    if bound is None:
        return {"en": "No active unit tests", "fr": "Aucun TU actif"}
    return {
        "en": f"> {bound:,} active unit tests",
        "fr": "+ de " + f"{bound:,}".replace(",", " ") + " TU actifs",
    }


def inventory(root: Path = ROOT, *, no_default_features: bool = False, features: str = "") -> dict:
    """Build/list tests, excluding ignored tests; no test body is run."""
    root = root.resolve()
    before = rust_fingerprint(root)
    command = ["cargo", "test", "--workspace", "--lib", "--bins", "--no-run", "--message-format=json", "--locked", "--offline"]
    if no_default_features:
        command.append("--no-default-features")
    if features:
        command.extend(["--features", features])
    artifacts = unit_executables(run(command, root, progress=True), crate_directories(root))
    rows = {}
    for artifact in artifacts:
        executable = artifact["executable"]
        # Pretty listing includes a summary, letting us refuse truncated output.
        # Terse listing suppresses it, including for an empty ignored-test list.
        all_tests = listed_tests(run([executable, "--list"], root))
        ignored = listed_tests(run([executable, "--list", "--ignored"], root))
        if not ignored <= all_tests:
            raise StatsError("Ignored test names were absent from the complete test inventory")
        row = rows.setdefault(artifact["crate"], {"name": artifact["crate"], "total": 0, "ignored": 0, "active": 0, "targets": []})
        row["total"] += len(all_tests)
        row["ignored"] += len(ignored)
        row["active"] += len(all_tests - ignored)
        row["targets"].append(artifact["target"])
    after = rust_fingerprint(root)
    if before != after:
        raise StatsError("Rust sources or Cargo inputs changed during the unit-test inventory; run it again")
    rustc = run(["rustc", "-Vv"], root).strip()
    host = re.search(r"(?m)^host: (.+)$", rustc)
    active = sum(row["active"] for row in rows.values())
    return {
        "kind": "libtest_inventory", "total": sum(row["total"] for row in rows.values()),
        "ignored": sum(row["ignored"] for row in rows.values()), "active": active,
        "lower_bound": lower_bound(active), "display": public_labels(active),
        "by_crate": [rows[name] for name in sorted(rows)],
        "configuration": {"default_features": not no_default_features, "extra_features": features, "rustc_host": host[1] if host else None, "platform": sys.platform},
        "provenance": {
            "collected_at": utc_now(), "rust_source_fingerprint": before,
            "rustc_version": rustc, "cargo_version": run(["cargo", "--version"], root).strip(),
            "build_command": command, "list_arguments": ["--list"],
            "ignored_list_arguments": ["--list", "--ignored"],
            "executed_tests": False,
        },
    }


def load_test_inventory(path: Path, root: Path = ROOT) -> dict:
    """Refuse stale or malformed cached counts instead of publishing them as current."""
    try:
        report = json.loads(read_bytes(path))
    except (ValueError, TypeError) as error:
        raise StatsError(f"Invalid statistics JSON: {path}") from error
    unit = report.get("unit_tests") if isinstance(report, dict) and report.get("schema_version") == 1 else None
    if not isinstance(unit, dict) or unit.get("kind") != "libtest_inventory":
        raise StatsError("Statistics report has no Rust unit-test inventory")
    for field in ("active", "total", "ignored"):
        if isinstance(unit.get(field), bool) or not isinstance(unit.get(field), int) or unit[field] < 0:
            raise StatsError(f"Invalid unit-test {field} count")
    if unit["active"] + unit["ignored"] != unit["total"]:
        raise StatsError("Unit-test active/ignored totals disagree")
    provenance = unit.get("provenance")
    if not isinstance(provenance, dict) or provenance.get("executed_tests") is not False:
        raise StatsError("Unit-test inventory provenance is missing")
    collected = provenance.get("collected_at")
    try:
        timestamp = datetime.fromisoformat(collected.replace("Z", "+00:00")) if isinstance(collected, str) else None
    except ValueError as error:
        raise StatsError("Unit-test inventory collection date is invalid") from error
    if timestamp is None or timestamp.tzinfo is None:
        raise StatsError("Unit-test inventory needs a collection date with its timezone")
    configuration = unit.get("configuration")
    if not isinstance(configuration, dict) or not isinstance(configuration.get("default_features"), bool):
        raise StatsError("Unit-test inventory Cargo feature configuration is missing")
    if not isinstance(configuration.get("extra_features"), str) or not isinstance(configuration.get("platform"), str) or not configuration["platform"]:
        raise StatsError("Unit-test inventory platform or extra-feature configuration is missing")
    if configuration.get("rustc_host") is not None and (not isinstance(configuration["rustc_host"], str) or not configuration["rustc_host"]):
        raise StatsError("Unit-test inventory Rust host is invalid")
    if provenance.get("rust_source_fingerprint") != rust_fingerprint(root.resolve()):
        raise StatsError("Unit-test inventory is stale: Rust sources or Cargo inputs changed")
    if unit.get("lower_bound") != lower_bound(unit["active"]) or unit.get("display") != public_labels(unit["active"]):
        raise StatsError("Unit-test public labels disagree with the active inventory")
    return unit


def text_report(report: dict) -> str:
    source = report["source"]
    lines = [f"Authored source: {source['total']['physical_lines']:,} physical lines in {source['total']['files']:,} files", LINE_DEFINITION, "", "Crate                     Production      Tests   Examples      Total"]
    for crate in source["crates"]:
        lines.append(f"{crate['name']:<25} {crate['production']['physical_lines']:>10,} {crate['tests']['physical_lines']:>10,} {crate['examples']['physical_lines']:>10,} {crate['total']['physical_lines']:>10,}")
    lines.extend(["", "Commits reachable from HEAD: " + report["commits"]["display"]["en"]])
    unit = report["unit_tests"]
    lines.extend(["", unit["display"]["en"] if unit else "Active Rust unit tests: not inventoried (use --tests)"])
    if unit:
        lines.append(f"Inventory: {unit['configuration']['rustc_host']}, {'default' if unit['configuration']['default_features'] else 'no default'} features; test bodies were not executed.")
    lines.extend(["", "Exclusions:"] + [f"- {item}" for item in source["exclusions"]])
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="Repository root (default: this script's repository)")
    parser.add_argument("--json", action="store_true", help="Print the machine-readable report instead of the text summary")
    parser.add_argument("--output", type=Path, help="Save the full JSON report; parent directory must already exist")
    parser.add_argument("--tests", action="store_true", help="Build/list active Rust lib/bin unit tests offline, without executing test bodies")
    parser.add_argument("--test-inventory", type=Path, help="Reuse unit-test data from a source-matching JSON report")
    parser.add_argument("--no-default-features", action="store_true", help="Inventory tests without default Cargo features (requires --tests)")
    parser.add_argument("--features", default="", help="Extra Cargo features for the unit-test inventory (requires --tests)")
    arguments = parser.parse_args()
    if arguments.tests and arguments.test_inventory:
        parser.error("--tests and --test-inventory are mutually exclusive")
    if not arguments.tests and (arguments.no_default_features or arguments.features):
        parser.error("Cargo feature options require --tests")
    try:
        report = collect(arguments.root)
        if arguments.tests:
            report["unit_tests"] = inventory(arguments.root, no_default_features=arguments.no_default_features, features=arguments.features)
        elif arguments.test_inventory:
            report["unit_tests"] = load_test_inventory(arguments.test_inventory, arguments.root)
        encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
        if arguments.output:
            if arguments.output.is_symlink():
                raise StatsError("Refusing to write a statistics report through a symlink")
            arguments.output.write_text(encoded, encoding="utf-8")
        print(encoded if arguments.json else text_report(report), end="" if arguments.json else "\n")
        return 0
    except (StatsError, OSError) as error:
        print(f"Project statistics failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
