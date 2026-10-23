"""Bounded real stdout PTY capture for literal album and credits paths."""

import errno
import json
import os
import pty
import select
import subprocess
import sys
import time


binary, directory, command, title = sys.argv[1:]
environment = dict(os.environ, AEDE_HOME=directory, TERM="xterm")
environment.pop("NO_COLOR", None)
for name in list(environment):
    if name.startswith("AEDE_DELEGATED_"):
        environment.pop(name)

master, slave = pty.openpty()
child = None
try:
    stdout_is_terminal = os.isatty(slave)
    child = subprocess.Popen(
        [binary, command, title],
        env=environment,
        stdin=subprocess.DEVNULL,
        stdout=slave,
        stderr=subprocess.PIPE,
    )
    os.close(slave)
    slave = None
    streams = {master: bytearray(), child.stderr.fileno(): bytearray()}
    pending = set(streams)
    deadline = time.monotonic() + 10
    while pending:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise AssertionError(f"{command} terminal output timed out: {streams!r}")
        ready, _, _ = select.select(list(pending), [], [], remaining)
        if not ready:
            raise AssertionError(f"{command} terminal output timed out: {streams!r}")
        for descriptor in ready:
            try:
                chunk = os.read(descriptor, 65536)
            except OSError as error:
                # Linux reports EIO when the last slave closes; macOS sends EOF.
                if descriptor != master or error.errno != errno.EIO:
                    raise
                chunk = b""
            if chunk:
                streams[descriptor].extend(chunk)
            else:
                pending.remove(descriptor)
    child.wait(timeout=max(0.01, deadline - time.monotonic()))
    print(json.dumps({
        "code": child.returncode,
        "stdout_is_terminal": stdout_is_terminal,
        "stdout": streams[master].decode(),
        "stderr": streams[child.stderr.fileno()].decode(),
    }))
finally:
    if child is not None:
        if child.poll() is None:
            child.kill()
        child.wait(timeout=10)
        if child.stderr is not None:
            child.stderr.close()
    os.close(master)
    if slave is not None:
        os.close(slave)
