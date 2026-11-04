#!/usr/bin/env python3
"""Real interactive pseudo-terminal acceptance for the pipi control center."""
import os
import pty
import select
import subprocess
import sys
import termios
import time

BINARY = sys.argv[1] if len(sys.argv) > 1 else "./target/release/pipi"


class Session:
    def __init__(self, args, extra_env=None):
        self.master, self.slave = pty.openpty()
        self.saved = termios.tcgetattr(self.slave)
        env = dict(os.environ)
        env["TERM"] = "xterm-256color"
        env.pop("NO_COLOR", None)
        env.update(extra_env or {})
        self.proc = subprocess.Popen(
            [BINARY] + args, stdin=self.slave, stdout=self.slave,
            stderr=self.slave, env=env,
        )
        self.seen = b""

    def until(self, marker, seconds=8):
        needle = marker.encode() if isinstance(marker, str) else marker
        deadline = time.monotonic() + seconds
        while needle not in self.seen and time.monotonic() < deadline:
            readable, _, _ = select.select(
                [self.master], [], [], min(0.2, max(0, deadline - time.monotonic()))
            )
            if readable:
                try:
                    block = os.read(self.master, 32768)
                except OSError:
                    break
                self.seen += block
                if len(self.seen) > 200000:
                    self.seen = self.seen[-180000:]
        assert needle in self.seen, (marker, self.seen[-2500:])

    def press(self, keys):
        os.write(self.master, keys)

    def finish(self):
        try:
            self.proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            raise AssertionError("pipi interactive menu did not terminate")
        assert self.proc.returncode == 0, (self.proc.returncode, self.seen[-1000:])
        actual = termios.tcgetattr(self.slave)
        assert actual == self.saved, ("termios was not restored", self.saved, actual)
        assert b"\x1b[?1049h" in self.seen, "did not enter alternate screen"
        # Capture final output emitted during drop/restore.
        for _ in range(10):
            readable, _, _ = select.select([self.master], [], [], 0.05)
            if readable:
                try:
                    self.seen += os.read(self.master, 32768)
                except OSError:
                    break
        assert b"\x1b[?1049l" in self.seen, "did not restore terminal screen"

    def close(self):
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait(timeout=2)
        os.close(self.master)
        os.close(self.slave)


def case(args, actions, env=None, expect_color=True):
    session = Session(args, env)
    try:
        session.until("NETWORK CONTROL CENTER")
        if expect_color:
            assert b"\x1b[1;36m" in session.seen, "expected colored panel"
        else:
            assert b"\x1b[1;36m" not in session.seen, "NO_COLOR was ignored"
        actions(session)
        session.finish()
    finally:
        session.close()


def basic(s):
    s.press(b"q")


case([], basic)
print("PASS: pipi without host enters colored menu, exits and restores termios")


def trace_form(s):
    s.press(b"5")
    s.until("TRACE / FORM")
    s.until("Max hops")
    s.press(b"\x1b")
    s.until("NETWORK CONTROL CENTER")
    s.press(b"q")


case(["tui"], trace_form)
print("PASS: pipi tui opens keyboard menu and trace configuration")


def active_confirmation(s):
    s.press(b"8")
    s.until("ARP SCAN / FORM")
    s.press(b"\t")
    s.press(b"eth0")
    s.press(b"\x12")  # Ctrl-R start, no network traffic before confirmation
    s.until("ACTIVE ARP / CONFIRM")
    assert b"ACTIVE ARP / CONFIRM" in s.seen
    s.press(b"n")
    s.until("cancelled")
    assert b"ARP SCAN / LIVE" not in s.seen
    s.press(b"\x1b")
    s.until("NETWORK CONTROL CENTER")
    s.press(b"q")


case([], active_confirmation)
print("PASS: active ARP requires second confirmation; cancel never starts scan")


case(["tui", "--no-color"], basic, expect_color=False)
case([], basic, {"NO_COLOR": "1"}, expect_color=False)
print("PASS: --no-color and NO_COLOR disable ANSI color, preserve UI/restore")


def ping_loopback(s):
    s.press(b"1")
    s.until("PING / FORM")
    s.press(b"\x12")  # Ctrl-R starts safe four-probe loopback
    s.until("PING / LIVE")
    s.until("127.0.0.1")
    s.press(b"q")
    s.until("PING / FORM")
    s.press(b"\x1b")
    s.until("NETWORK CONTROL CENTER")
    s.press(b"q")


case([], ping_loopback)
print("PASS: menu launches loopback ping, streams results, supports stop/back")


non_tty = subprocess.run([BINARY], input=b"", stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, timeout=4)
assert non_tty.returncode == 2, non_tty
assert b"terminal" in non_tty.stderr or b"TTY" in non_tty.stderr
assert b"\x1b[?1049h" not in non_tty.stdout
print("PASS: non-interactive pipi without host rejects UI without ANSI")
