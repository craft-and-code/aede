#!/usr/bin/env python3
"""Run only library/acceptance tests; record native architecture and static musl linkage."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import struct
import subprocess


def execute(command, **kwargs):
    print("+", " ".join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), check=True, **kwargs)


def audit(binary, target):
    data = binary.read_bytes()
    if "windows" in target:
        if data[:2] != b"MZ":
            raise ValueError("expected a Windows test executable")
        offset = struct.unpack_from("<I", data, 0x3c)[0]
        if data[offset:offset + 4] != b"PE\0\0":
            raise ValueError("invalid PE signature")
        machine = struct.unpack_from("<H", data, offset + 4)[0]
        expected = 0xaa64 if target.startswith("aarch64") else 0x8664
        if machine != expected:
            raise ValueError("test executable does not match the native Windows target")
        return {"format": "PE", "machine": machine}
    if target.endswith("apple-darwin"):
        header = execute(["otool", "-hv", binary], capture_output=True, text=True).stdout
        expected = "ARM64" if target.startswith("aarch64") else "X86_64"
        if expected not in header:
            raise ValueError("test executable does not match the native macOS target")
        return {"format": "Mach-O", "machine": expected}
    header = execute(["readelf", "-h", binary], capture_output=True, text=True).stdout
    expected = "AArch64" if target.startswith("aarch64") else "Advanced Micro Devices X86-64"
    if expected not in header:
        raise ValueError("test executable does not match the native Linux target")
    result = {"format": "ELF", "machine": expected}
    if target.endswith("musl"):
        segments = execute(["readelf", "-l", binary], capture_output=True, text=True).stdout
        dynamic = execute(["readelf", "-d", binary], capture_output=True, text=True).stdout
        if "INTERP" in segments or "(NEEDED)" in dynamic:
            raise ValueError("musl acceptance binary requires a loader/shared library")
        result["static"] = True
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--aede", type=Path, required=True)
    parser.add_argument("--flaccompagnon", type=Path, required=True)
    parser.add_argument("--phemios", type=Path, required=True)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--acceptance", type=Path, required=True)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()
    report = {"target": args.target, "host": platform.machine(), "status": "failed"}
    try:
        report["rustc"] = execute(["rustc", "-Vv"], capture_output=True, text=True).stdout
        for name in ["aede", "flaccompagnon", "phemios"]:
            repo = getattr(args, name)
            report[name] = {"commit": execute(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip(), "working_tree_dirty": bool(execute(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout.strip())}
        report["acceptance_lock_sha256"] = hashlib.sha256((args.acceptance / "Cargo.lock").read_bytes()).hexdigest()
        if not args.offline:
            execute(["cargo", "fetch", "--locked", "--manifest-path", args.acceptance / "Cargo.toml"])
            execute(["cargo", "fetch", "--locked", "--manifest-path", args.aede / "Cargo.toml"])

        common = ["--locked", "--offline"]
        host_target = next(line.split(": ", 1)[1] for line in report["rustc"].splitlines() if line.startswith("host: "))
        if args.target != host_target:
            common += ["--target", args.target]
        execute(["cargo", "test", "--manifest-path", args.aede / "Cargo.toml", "-p", "aede-audio", "--features", "ape-pcm16", "--lib", *common, "native::"])
        execute(["cargo", "test", "--manifest-path", args.acceptance / "Cargo.toml", "--test", "acceptance", *common, "--", "--nocapture"])
        output = execute(["cargo", "test", "--no-run", "--message-format=json", "--manifest-path", args.acceptance / "Cargo.toml", "--test", "acceptance", *common], capture_output=True, text=True).stdout
        artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
        binaries = [Path(v["executable"]) for v in artifacts if v.get("reason") == "compiler-artifact" and v.get("executable") and v.get("target", {}).get("name") == "acceptance"]
        if len(binaries) != 1:
            raise ValueError("expected exactly one acceptance test executable")
        report["binary"] = audit(binaries[0], args.target)
        report["status"] = "passed"
    finally:
        (args.acceptance / "platform-report.json").write_text(json.dumps(report, indent=2) + "\n")
