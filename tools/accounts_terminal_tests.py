#!/usr/bin/env python3
"""Exercise masked account prompts in real Unix pseudo-terminals, offline."""

import argparse
import json
import os
from pathlib import Path
import select
import shlex
import shutil
import subprocess
import tempfile
import threading
import time
import unittest

if os.name != "nt":
    import pty
    import termios


class Terminal:
    def __init__(self, home, *arguments, input_flags=None, environment=None):
        self.master, self.slave = pty.openpty()
        os.set_blocking(self.master, False)
        if input_flags is not None:
            mode = termios.tcgetattr(self.slave)
            enabled, disabled = input_flags
            mode[0] = (mode[0] | enabled) & ~disabled
            termios.tcsetattr(self.slave, termios.TCSANOW, mode)
        self.original_mode = termios.tcgetattr(self.slave)
        self.transcript = b""
        self.child = subprocess.Popen(
            [str(BINARY), "accounts", *arguments],
            stdin=self.slave,
            stderr=self.slave,
            stdout=subprocess.PIPE,
            env={**os.environ, "AEDE_HOME": str(home), "NO_COLOR": "1", "TERM": "xterm", **(environment or {})},
        )

    def read_output(self):
        try:
            output = os.read(self.master, 65536)
        except BlockingIOError:
            return False
        self.transcript += output
        return bool(output)

    def drain_output(self):
        while self.read_output():
            pass

    def wait_for(self, text):
        expected = text.encode()
        deadline = time.monotonic() + 10
        while expected not in self.transcript:
            if time.monotonic() >= deadline:
                raise AssertionError(f"missing {text!r}: {self.transcript!r}")
            ready, _, _ = select.select([self.master], [], [], 0.05)
            if ready:
                self.read_output()
            elif self.child.poll() is not None:
                raise AssertionError(f"process exited before {text!r}: {self.transcript!r}")

    def send(self, value):
        # Synchronize with the key reader, rather than sending before it has
        # disabled echo and letting the test itself leak its fake password.
        deadline = time.monotonic() + 10
        while termios.tcgetattr(self.slave)[3] & termios.ECHO:
            if time.monotonic() >= deadline or self.child.poll() is not None:
                raise AssertionError("masked reader did not disable echo")
            time.sleep(0.005)
        pending = memoryview(value.encode() if isinstance(value, str) else value)
        written = 0
        deadline = time.monotonic() + 10
        while written < len(pending):
            ready, writable, _ = select.select([self.master], [self.master], [], 0.05)
            if ready:
                self.read_output()
            if self.child.poll() is not None:
                self.drain_output()
                raise AssertionError(
                    f"process exited after {written}/{len(pending)} input bytes: {self.transcript!r}"
                )
            if time.monotonic() >= deadline:
                raise AssertionError(
                    f"terminal input stalled after {written}/{len(pending)} bytes: {self.transcript!r}"
                )
            if writable:
                try:
                    count = os.write(self.master, pending[written:])
                except BlockingIOError:
                    continue
                if count == 0:
                    raise AssertionError("terminal input write made no progress")
                written += count

    def finish(self, restored=True):
        deadline = time.monotonic() + 10
        while self.child.poll() is None:
            if time.monotonic() >= deadline:
                raise AssertionError(f"process did not finish: {self.transcript!r}")
            if select.select([self.master], [], [], 0.05)[0]:
                self.read_output()
        stdout, _ = self.child.communicate(timeout=10)
        self.drain_output()
        final_mode = termios.tcgetattr(self.slave)
        if not restored:
            if final_mode[3] & termios.ECHO:
                raise AssertionError("echo was restored before queued password input could be discarded")
            return self.child.returncode, stdout
        # macOS sets PENDIN when moving queued input back into canonical mode;
        # it is transient driver state, not a changed echo/edit/signal setting.
        final_mode[3] &= ~getattr(termios, "PENDIN", 0)
        self.original_mode[3] &= ~getattr(termios, "PENDIN", 0)
        if final_mode != self.original_mode:
            raise AssertionError(f"terminal mode was not restored: {self.original_mode!r} -> {final_mode!r}")
        return self.child.returncode, stdout

    def __enter__(self):
        return self

    def __exit__(self, *_):
        if self.child.poll() is None:
            self.child.kill()
        self.child.communicate()
        # Failed cleanup intentionally retains masking. Restore only this
        # private fixture terminal, after discarding its queued fake secrets.
        termios.tcflush(self.slave, termios.TCIFLUSH)
        termios.tcsetattr(self.slave, termios.TCSANOW, self.original_mode)
        os.close(self.master)
        os.close(self.slave)


@unittest.skipIf(os.name == "nt", "Unix pseudo-terminal checks; key logic is tested by Cargo on Windows")
class AccountTerminalTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="aede_account_terminal_")
        self.addCleanup(self.directory.cleanup)
        self.home = Path(self.directory.name)
        self.accounts = self.home / "accounts.json"

    def redirected(self, *arguments):
        return subprocess.run(
            [str(BINARY), "accounts", *arguments, "--password-stdin"],
            input=b"a long scripted passphrase\n",
            capture_output=True,
            timeout=10,
            env={**os.environ, "AEDE_HOME": str(self.home), "NO_COLOR": "1"},
        )

    def assert_hidden(self, terminal, *secrets):
        for secret in secrets:
            self.assertNotIn(secret.encode(), terminal.transcript)

    def enter(self, terminal, password, confirmation=None):
        terminal.wait_for("Password: ")
        terminal.send(password + "\r")
        terminal.wait_for("Confirm password: ")
        terminal.send((password if confirmation is None else confirmation) + "\r")

    def test_init_create_and_reset_are_masked_with_clean_json(self):
        secret = " a long é音🔑 passphrase "
        for arguments in [("init", "admin"), ("create", "alice", "user"), ("password", "alice")]:
            with self.subTest(arguments=arguments), Terminal(self.home, *arguments, "--json") as terminal:
                self.enter(terminal, secret)
                code, stdout = terminal.finish()
                self.assertEqual(code, 0, terminal.transcript)
                self.assertIsInstance(json.loads(stdout), list)
                self.assert_hidden(terminal, secret)
                self.assertNotIn(secret.encode(), stdout)
                self.assertNotIn(secret.encode(), self.accounts.read_bytes())

    def test_unicode_backspace_and_line_clear_edit_without_echo(self):
        secret = "a long é音🔑 passphrase"
        with Terminal(self.home, "init", "admin") as terminal:
            terminal.wait_for("Password: ")
            terminal.send("discard me\x15" + secret + "🧩\x7f\r")
            terminal.wait_for("Confirm password: ")
            terminal.send(secret + "\r")
            code, _ = terminal.finish()
            self.assertEqual(code, 0, terminal.transcript)
            self.assert_hidden(terminal, secret, "discard me", "🧩")

    def test_fragmented_unicode_preserves_the_complete_password(self):
        secret = "a long é音🔑 passphrase"
        with Terminal(self.home, "init", "admin") as terminal:
            terminal.wait_for("Password: ")
            for byte in secret.encode():
                terminal.send(bytes([byte]))
                time.sleep(0.001)
            terminal.send("\r")
            terminal.wait_for("Confirm password: ")
            terminal.send(secret + "\r")
            code, _ = terminal.finish()
            self.assertEqual(code, 0, terminal.transcript)
            self.assert_hidden(terminal, secret)

    def test_navigation_and_bracketed_paste_sequences_do_not_change_the_password(self):
        secret = "a long é音🔑 passphrase"
        with Terminal(self.home, "init", "admin") as terminal:
            terminal.wait_for("Password: ")
            terminal.send(b"\x1b[200~" + secret.encode() + b"\x1b[D\x1b[3~\x1b[201~\r")
            terminal.wait_for("Confirm password: ")
            terminal.send(secret + "\r")
            code, _ = terminal.finish()
            self.assertEqual(code, 0, terminal.transcript)
            self.assert_hidden(terminal, secret)

    def test_invalid_unicode_restores_the_terminal_without_saving(self):
        with Terminal(self.home, "init", "admin") as terminal:
            terminal.wait_for("Password: ")
            terminal.send(b"\xff")
            code, stdout = terminal.finish()
            self.assertNotEqual(code, 0)
            self.assertEqual(stdout, b"")
            self.assertIn(b"not valid UTF-8", terminal.transcript)
        self.assertFalse(self.accounts.exists())

    def test_rejected_or_cancelled_paste_does_not_echo_its_queued_tail(self):
        tail = b"PRIVATE_PASTE_TAIL" * 200
        cases = [
            ("interrupt", b"\x03"),
            ("end_of_input", b"\x04"),
            ("control_character", b"\t"),
            ("invalid_utf8", b"\xff"),
            ("oversized_csi", b"\x1b[" + b"0" * 17),
            ("invalid_alt_utf8", b"\x1b\xc3\x28"),
        ]
        for label, invalid in cases:
            with self.subTest(sequence=label), Terminal(self.home, "init", "admin") as terminal:
                terminal.wait_for("Password: ")
                terminal.send(b"a" + invalid + tail + b"\r")
                code, stdout = terminal.finish()
                self.assertNotEqual(code, 0)
                self.assertEqual(stdout, b"")
                self.assert_hidden(terminal, "PRIVATE_PASTE_TAIL")
        self.assertFalse(self.accounts.exists())

    def test_confirmation_discards_queued_suffix_on_success_and_mismatch(self):
        self.assertEqual(self.redirected("init", "admin").returncode, 0)
        secret = "a long terminal passphrase"
        tail = b"PRIVATE_PASTE_TAIL" * 200
        for matches in [False, True]:
            before = self.accounts.read_bytes()
            confirmation = secret if matches else "a different terminal passphrase"
            with self.subTest(matches=matches), Terminal(self.home, "password", "admin", "--json") as terminal:
                terminal.wait_for("Password: ")
                terminal.send(secret + "\r")
                terminal.wait_for("Confirm password: ")
                terminal.send(confirmation.encode() + b"\r" + tail + b"\r")
                code, stdout = terminal.finish()
                self.assert_hidden(terminal, secret, confirmation, "PRIVATE_PASTE_TAIL")
                self.assertNotIn(secret.encode(), stdout)
                self.assertNotIn(confirmation.encode(), stdout)
                if matches:
                    self.assertEqual(code, 0, terminal.transcript)
                    self.assertIsInstance(json.loads(stdout), list)
                    self.assertNotEqual(self.accounts.read_bytes(), before)
                else:
                    self.assertNotEqual(code, 0)
                    self.assertEqual(stdout, b"")
                    self.assertIn(b"passwords do not match", terminal.transcript)
                    self.assertEqual(self.accounts.read_bytes(), before)

    def test_inherited_input_translation_is_disabled_then_restored(self):
        self.assertEqual(self.redirected("init", "admin").returncode, 0)
        secret = "a long é音🔑 passphrase"
        modes = [
            ("strip_and_flow_control", termios.ISTRIP | termios.IXON | termios.IXOFF, 0),
            ("carriage_returns_and_marks", termios.IGNCR | termios.INLCR | termios.PARMRK, termios.ICRNL),
        ]
        for label, enabled, disabled in modes:
            with self.subTest(mode=label), Terminal(
                self.home, "password", "admin", input_flags=(enabled, disabled)
            ) as terminal:
                self.enter(terminal, secret)
                code, _ = terminal.finish()
                self.assertEqual(code, 0, terminal.transcript)
                self.assert_hidden(terminal, secret)

    def test_failed_queue_cleanup_retains_masking_and_refuses_account_changes(self):
        actual_stty = shutil.which("stty")
        self.assertIsNotNone(actual_stty)
        stub_directory = self.home / "stub-bin"
        stub_directory.mkdir()
        stub = stub_directory / "stty"
        stub.write_text(
            "#!/bin/sh\n"
            'if [ "$#" -eq 4 ] && [ "$1" = min ] && [ "$2" = 0 ] '
            '&& [ "$3" = time ] && [ "$4" = 1 ]; then\n'
            "    exit 1\n"
            "fi\n"
            f"exec {shlex.quote(actual_stty)} \"$@\"\n"
        )
        stub.chmod(0o700)
        with Terminal(
            self.home,
            "init",
            "admin",
            environment={"PATH": f"{stub_directory}{os.pathsep}{os.environ.get('PATH', '')}"},
        ) as terminal:
            terminal.wait_for("Password: ")
            terminal.send(b"\xffPRIVATE_PASTE_TAIL\r")
            code, stdout = terminal.finish(restored=False)
            self.assertNotEqual(code, 0)
            self.assertEqual(stdout, b"")
            self.assertIn(b"terminal echo remains disabled", terminal.transcript)
            self.assert_hidden(terminal, "PRIVATE_PASTE_TAIL")
        self.assertFalse(self.accounts.exists())

    def test_flow_control_characters_are_refused_instead_of_swallowed(self):
        for control in [b"\x11", b"\x13"]:
            with self.subTest(control=control), Terminal(self.home, "init", "admin") as terminal:
                terminal.wait_for("Password: ")
                terminal.send(control + b"PRIVATE_PASTE_TAIL" * 100 + b"\r")
                code, _ = terminal.finish()
                self.assertNotEqual(code, 0)
                self.assertIn(b"control characters", terminal.transcript)
                self.assert_hidden(terminal, "PRIVATE_PASTE_TAIL")
        self.assertFalse(self.accounts.exists())

    def test_echo_stays_disabled_between_keys_and_confirmation(self):
        with Terminal(self.home, "init", "admin") as terminal:
            terminal.wait_for("Password: ")
            terminal.send("a")
            stop = threading.Event()
            exposed = threading.Event()

            def observe():
                while not stop.is_set():
                    if termios.tcgetattr(terminal.slave)[3] & termios.ECHO:
                        exposed.set()

            observer = threading.Thread(target=observe)
            observer.start()
            try:
                for _ in range(50):
                    terminal.send("b")
                    time.sleep(0.001)
                terminal.send("\r")
                terminal.wait_for("Confirm password: ")
                terminal.send("a" + "b" * 50)
            finally:
                stop.set()
                observer.join(timeout=10)
            terminal.send("\r")
            code, _ = terminal.finish()
            self.assertEqual(code, 0, terminal.transcript)
            self.assertFalse(exposed.is_set(), "echo became enabled during password entry")

    def test_mismatch_and_cancellation_preserve_credentials_and_terminal(self):
        self.assertEqual(self.redirected("init", "admin").returncode, 0)
        before = self.accounts.read_bytes()
        secret = "another secret passphrase"
        with Terminal(self.home, "password", "admin", "--json") as terminal:
            self.enter(terminal, secret, "different secret passphrase")
            code, stdout = terminal.finish()
            self.assertNotEqual(code, 0)
            self.assertEqual(stdout, b"")
            self.assertIn(b"passwords do not match", terminal.transcript)
            self.assert_hidden(terminal, secret, "different secret passphrase")
        for cancel in [b"\x03", b"\x04", b"\x1b"]:
            for prompt in ["Password: ", "Confirm password: "]:
                with self.subTest(cancel=cancel, prompt=prompt), Terminal(self.home, "password", "admin") as terminal:
                    terminal.wait_for("Password: ")
                    if prompt == "Confirm password: ":
                        terminal.send(secret + "\r")
                        terminal.wait_for(prompt)
                    terminal.send(secret.encode() + cancel)
                    code, _ = terminal.finish()
                    self.assertNotEqual(code, 0)
                    self.assertIn(b"cancelled", terminal.transcript)
                    self.assert_hidden(terminal, secret)
        self.assertEqual(self.accounts.read_bytes(), before)

    def test_overlong_paste_is_refused_without_truncation_or_echo(self):
        with Terminal(self.home, "init", "admin") as terminal:
            terminal.wait_for("Password: ")
            terminal.send("é" * 513 + "\r")
            code, _ = terminal.finish()
            self.assertNotEqual(code, 0)
            self.assertIn(b"at most 1024", terminal.transcript)
            self.assert_hidden(terminal, "é")
        self.assertFalse(self.accounts.exists())

    def test_prompt_holds_no_writer_lock_and_rechecks_concurrent_initialization(self):
        with Terminal(self.home, "init", "admin") as terminal:
            terminal.wait_for("Password: ")
            result = self.redirected("init", "other")
            self.assertEqual(result.returncode, 0, result.stderr)
            before = self.accounts.read_bytes()
            self.enter(terminal, "a long interactive passphrase")
            code, _ = terminal.finish()
            self.assertNotEqual(code, 0)
            self.assertIn(b"already initialized", terminal.transcript)
        self.assertEqual(self.accounts.read_bytes(), before)

    def test_explicit_stdin_flag_refuses_terminal_input_before_prompting(self):
        with Terminal(self.home, "init", "admin", "--password-stdin") as terminal:
            code, _ = terminal.finish()
            self.assertNotEqual(code, 0)
            self.assertIn(b"requires redirected input", terminal.transcript)
            self.assertNotIn(b"Password: ", terminal.transcript)
        self.assertFalse(self.accounts.exists())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    arguments, remaining = parser.parse_known_args()
    BINARY = arguments.binary.resolve(strict=True)
    unittest.main(argv=[__file__, *remaining])
