"""Bounded Unix pseudo-terminal probe used by playback_transport.rs."""

import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time


binary, directory, selection, scenario = sys.argv[1:]
directory = Path(directory)
bin_directory = directory / "bin"
bin_directory.mkdir()
sink = bin_directory / "ffplay"
# Throttle only the disposable sink, making the producer poll controls while
# reading real PCM. Per-process files distinguish discarded output after seek.
sink.write_text(
    "#!/usr/bin/env python3\n"
    "import os, pathlib, threading, time\n"
    "root = pathlib.Path(os.environ['AEDE_TEST_RUN'])\n"
    "def heartbeat():\n"
    "    counter = 0\n"
    "    while True:\n"
    "        counter += 1\n"
    "        (root / ('heartbeat.' + str(os.getpid()))).write_text(str(counter))\n"
    "        time.sleep(0.005)\n"
    "threading.Thread(target=heartbeat, daemon=True).start()\n"
    "with (root / 'streams').open('a') as starts:\n"
    "    starts.write(str(os.getpid()) + '\\n')\n"
    "with (root / ('pcm.' + str(os.getpid()))).open('ab', buffering=0) as stream:\n"
    "    while True:\n"
    "        chunk = os.read(0, 4096)\n"
    "        if not chunk:\n"
    "            break\n"
    "        stream.write(chunk)\n"
    "        time.sleep(0.005)\n"
)
sink.chmod(0o700)
arguments = [binary, "play", selection, "--data", str(directory / "data")]
if scenario.startswith("repeat-"):
    arguments += ["--normalize", "off", "--repeat", scenario.removeprefix("repeat-")]
elif scenario.startswith("initial-seek-next"):
    arguments += ["--normalize", "off", "--seek", "0.02"]
elif scenario.startswith("lyrics-"):
    arguments += ["--normalize", "off", "--lyrics"]
    if scenario == "lyrics-repeat":
        arguments += ["--repeat", "one"]
environment = {
    **os.environ,
    "PATH": str(bin_directory) + os.pathsep + os.environ.get("PATH", ""),
    "AEDE_AUDIO_BACKEND": "ffplay",
    "AEDE_TEST_RUN": str(directory),
    "NO_COLOR": "1",
    "TERM": "xterm",
}
environment.pop("AEDE_DELEGATED_CHILD", None)
environment.pop("AEDE_DELEGATED_DATA_DIR", None)
master, slave = pty.openpty()
original_mode = termios.tcgetattr(slave)
os.set_blocking(master, False)
if scenario.startswith("initial-seek-next"):
    # This is queued before Aède starts its key reader or any source decoding.
    os.write(master, b"nn" if scenario.endswith("-twice") else b"n")
child = subprocess.Popen(arguments, stdin=slave, stdout=slave, stderr=slave,
                         env=environment, start_new_session=True)
transcript = bytearray()
started = time.monotonic()


def read_output():
    if select.select([master], [], [], 0.01)[0]:
        try:
            transcript.extend(os.read(master, 65536))
        except BlockingIOError:
            pass


def wait_for(predicate):
    deadline = time.monotonic() + 15
    while not predicate():
        read_output()
        if child.poll() is not None:
            raise AssertionError(f"player exited early ({child.returncode}): {bytes(transcript)!r}")
        if time.monotonic() >= deadline:
            raise AssertionError(f"terminal progress timed out: {bytes(transcript)!r}")


def playing():
    clean = re.sub(rb"\x1b\[[0-9;?]*[A-Za-z]", b"", bytes(transcript))
    return re.findall(rb"Playing: [^\r\n]*", clean)


def streams():
    path = directory / "streams"
    return path.read_text().splitlines() if path.exists() else []


def stream_ready(count):
    ids = streams()
    if len(ids) < count:
        return False
    path = directory / ("pcm." + ids[count - 1])
    return path.exists() and path.stat().st_size >= 8192


def paused_probe():
    # A heartbeat runs independently of pipe reads, so starving the PCM pipe
    # alone cannot satisfy the check that the output process itself is paused.
    previous = None
    last_change = time.monotonic()

    def stopped():
        nonlocal previous, last_change
        identifiers = streams()
        if not identifiers:
            return False
        path = directory / ("heartbeat." + identifiers[-1])
        value = path.read_text() if path.exists() else ""
        if not value or value != previous:
            previous = value
            last_change = time.monotonic()
            return False
        return time.monotonic() - last_change >= 0.1

    return stopped


try:
    wait_for(lambda: b"Controls:" in transcript)
    if scenario == "repeat-one":
        wait_for(lambda: len(playing()) >= 3)
        assert all(b"first" in label for label in playing()[:3]), playing()
    elif scenario == "repeat-all":
        wait_for(lambda: len(playing()) >= 5)
        expected = [b"first", b"second", b"first", b"second", b"first"]
        assert all(name in label for name, label in zip(expected, playing()[:5])), playing()
    elif scenario == "seek":
        wait_for(lambda: stream_ready(1))
        os.write(master, b"]")
        wait_for(lambda: stream_ready(2))
        os.write(master, b"[")
        wait_for(lambda: stream_ready(3))
    elif scenario == "seek-past-end":
        wait_for(lambda: stream_ready(1))
        os.write(master, b"]")
        wait_for(lambda: any(b"second" in label for label in playing()))
    elif scenario.startswith("initial-seek-next"):
        expected_name = b"third" if scenario.endswith("-twice") else b"second"
        wait_for(lambda: bool(playing()))
        assert all(expected_name in label for label in playing()), "each preloaded Next must skip one source before playback"
    elif scenario == "paused-modes":
        wait_for(lambda: stream_ready(1))
        os.write(master, b" ")
        output_paused = paused_probe()
        wait_for(output_paused)
        paused_output = directory / ("pcm." + streams()[0])
        paused_size = paused_output.stat().st_size
        os.write(master, b"r")
        wait_for(lambda: b"Repeat: one" in transcript)
        os.write(master, b"z")
        wait_for(lambda: b"Shuffle: random (future entries)" in transcript)
        os.write(master, b"z")
        wait_for(lambda: b"Shuffle: smart (future entries)" in transcript)
        os.write(master, b"r")
        wait_for(lambda: b"Repeat: all" in transcript)
        os.write(master, b"r")
        wait_for(lambda: b"Repeat: off" in transcript)
        os.write(master, b"z")
        wait_for(lambda: b"Shuffle: off (future entries)" in transcript)
        assert output_paused(), "changing modes must not resume the output"
        assert paused_output.stat().st_size == paused_size, "paused mode changes submit no new audio"
        assert len(streams()) == 1, "changing order must not reopen the current track"
        os.write(master, b" ")
        wait_for(lambda: paused_output.stat().st_size > paused_size)
    elif scenario == "lyrics-transport":
        wait_for(lambda: b"first-opening" in transcript and stream_ready(1))
        assert b"Timing estimated" in transcript, "ffplay must announce its estimated clock"
        os.write(master, b" ")
        output_paused = paused_probe()
        wait_for(output_paused)
        paused_words = bytes(transcript).count(b"first-opening")
        os.write(master, b"r")
        wait_for(lambda: b"Repeat: one" in transcript)
        assert b"first-later" not in transcript, "pause cannot advance the lyric timeline"
        assert bytes(transcript).count(b"first-opening") == paused_words
        os.write(master, b" ")
        os.write(master, b"]")
        wait_for(lambda: b"first-later" in transcript and stream_ready(2))
        os.write(master, b"[")
        wait_for(lambda: bytes(transcript).count(b"first-opening") >= 2 and stream_ready(3))
        os.write(master, b"n")
        wait_for(lambda: b"second-opening" in transcript)
    elif scenario == "lyrics-repeat":
        wait_for(lambda: bytes(transcript).count(b"repeated-opening") >= 2)
        assert len(streams()) == 1, "compatible repeats retain the same output session"
    else:
        raise AssertionError(f"unknown scenario: {scenario}")
    if scenario != "seek-past-end" and not scenario.startswith("initial-seek-next"):
        os.write(master, b"q")
    deadline = time.monotonic() + 15
    while child.poll() is None:
        read_output()
        if time.monotonic() >= deadline:
            raise AssertionError(f"q did not stop playback: {bytes(transcript)!r}")
    read_output()
    assert child.returncode == 0, bytes(transcript)
    mode = termios.tcgetattr(slave)
    mode[3] &= ~getattr(termios, "PENDIN", 0)
    original_mode[3] &= ~getattr(termios, "PENDIN", 0)
    assert mode == original_mode, "transport must restore terminal input mode"
    samples = []
    output = bytearray()
    for identifier in streams():
        path = directory / ("pcm." + identifier)
        data = path.read_bytes() if path.exists() else b""
        output.extend(data)
        samples.append(struct.unpack("<f", data[:4])[0] if len(data) >= 4 else None)
    (directory / "output.pcm").write_bytes(output)
    print(json.dumps({"playing": [line.decode(errors="replace") for line in playing()],
                      "first_samples": samples,
                      "lyrics": [line.decode(errors="replace") for line in
                                 re.findall(rb"[^\r\n]*\xe2\x96\xb8[^\r\n]*", bytes(transcript))],
                      "elapsed_ms": int((time.monotonic() - started) * 1000)}))
finally:
    if child.poll() is None:
        os.killpg(child.pid, signal.SIGKILL)
    child.wait(timeout=5)
    termios.tcsetattr(slave, termios.TCSANOW, original_mode)
    os.close(master)
    os.close(slave)
