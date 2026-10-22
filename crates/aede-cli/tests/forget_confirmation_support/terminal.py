"""Bounded real-terminal confirmation probe; no simulated terminal flags."""

import json
import os
from pathlib import Path
import pty
import select
import subprocess
import sys
import time

binary, directory, command, answer = sys.argv[1:]
directory = Path(directory)


def snapshot():
    result = {}
    for name in ("catalog.json", "conclusions.json", "sources.json", "user.json",
                 "accounts.json", "parental.json"):
        path = directory / name
        result[name] = (path.read_bytes(), path.stat().st_mtime_ns) if path.exists() else None
    return result


before = snapshot()
environment = dict(os.environ, AEDE_HOME=str(directory), NO_COLOR="1")
environment.pop("AEDE_DELEGATED_CHILD", None)
environment.pop("AEDE_DELEGATED_DATA_DIR", None)
master, slave = pty.openpty()
child = None
try:
    child = subprocess.Popen([binary, command, "--forget"], env=environment,
                             stdin=slave, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    os.close(slave)
    slave = None
    output = bytearray()
    deadline = time.monotonic() + 10
    prompt = b'Type "yes" to confirm: '
    while prompt not in output:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([child.stdout], [], [], remaining)[0]:
            raise AssertionError(f"confirmation prompt timed out for {command}/{answer}: {output!r}")
        chunk = os.read(child.stdout.fileno(), 65536)
        if not chunk:
            raise AssertionError(f"command ended before confirmation: {output!r}")
        output.extend(chunk)
    assert snapshot() == before, "stores changed before confirmation"
    if answer == "disconnect":
        child.kill()
    else:
        os.write(master, b"\x04" if answer == "eof" else (answer + "\n").encode())
    tail, error = child.communicate(timeout=10)
    output.extend(tail)
    print(json.dumps({"code": child.returncode, "stdout": output.decode(),
                      "stderr": error.decode(), "unchanged_before_confirmation": True}))
finally:
    if child is not None and child.poll() is None:
        child.kill()
        child.communicate(timeout=10)
    os.close(master)
    if slave is not None:
        os.close(slave)
