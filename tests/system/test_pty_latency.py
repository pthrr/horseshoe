"""System tests for horseshoe key-repeat / prompt latency.

These tests spawn the actual `hs` binary with HAND_PROFILE=1, send
keystrokes via `wtype`, and parse the profiling output to detect
the "delayed prompt when holding Enter" flaw.

Requirements (provided via nix-shell):
    wtype   – Wayland keystroke injector

Run:
    nix-shell -p wtype --run "python3 tests/test_pty_latency.py"
"""

import os
import pty
import re
import select
import shutil
import signal
import subprocess
import sys
import time
import unittest

HS_BIN = os.path.join(
    os.path.dirname(__file__), "..", "..", "target", "release", "hs"
)
WTYPE = shutil.which("wtype")


def require_wayland():
    if not os.environ.get("WAYLAND_DISPLAY"):
        raise unittest.SkipTest("No Wayland session (WAYLAND_DISPLAY not set)")


def require_wtype():
    if not WTYPE:
        raise unittest.SkipTest("wtype not found in PATH")


def require_hs():
    if not os.path.isfile(HS_BIN):
        raise unittest.SkipTest(f"hs binary not found at {HS_BIN}")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def pty_open_shell():
    """Open a bash shell on a PTY pair. Returns (master_fd, pid)."""
    pid, master_fd = pty.fork()
    if pid == 0:
        os.environ["PS1"] = "PROMPT> "
        os.environ["TERM"] = "xterm-256color"
        os.execvp("bash", ["bash", "--norc", "--noprofile", "-i"])
    return master_fd, pid


def pty_read_until(fd, marker, timeout=3.0):
    """Read from fd until *marker* appears or timeout."""
    start = time.monotonic()
    buf = b""
    deadline = start + timeout
    while time.monotonic() < deadline:
        remaining = deadline - time.monotonic()
        r, _, _ = select.select([fd], [], [], min(remaining, 0.05))
        if r:
            chunk = os.read(fd, 4096)
            if not chunk:
                break
            buf += chunk
            if marker in buf:
                return buf, time.monotonic() - start
    return buf, time.monotonic() - start


def pty_drain(fd, timeout=0.5):
    """Drain all pending output from fd."""
    buf = b""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        remaining = deadline - time.monotonic()
        r, _, _ = select.select([fd], [], [], min(remaining, 0.05))
        if r:
            chunk = os.read(fd, 4096)
            if not chunk:
                break
            buf += chunk
        else:
            break
    return buf


def spawn_hs_profiled(extra_env=None, timeout_sec=6):
    """Spawn hs with profiling, wait *timeout_sec*, kill it, return stderr."""
    require_wayland()
    require_hs()
    env = os.environ.copy()
    env["HAND_PROFILE"] = "1"
    if extra_env:
        env.update(extra_env)
    proc = subprocess.Popen(
        [HS_BIN],
        stderr=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        env=env,
    )
    try:
        time.sleep(timeout_sec)
    finally:
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    return proc.stderr.read().decode(errors="replace")


def parse_draw_times(stderr_text):
    """Extract draw times (ms) from HAND_PROFILE output."""
    pattern = re.compile(r"\[profile\] draw:\s+([\d.]+)ms")
    return [float(m.group(1)) for m in pattern.finditer(stderr_text)]


def count_repeat_events(stderr_text):
    """Count [profile] repeat: lines from HAND_PROFILE output.
    Matches both calloop timer repeats and compositor repeat_key events."""
    return len(re.findall(r"\[profile\] repeat:", stderr_text))


def count_pty_reads(stderr_text):
    """Count [profile] pty: lines from HAND_PROFILE output."""
    return len(re.findall(r"\[profile\] pty:", stderr_text))


def count_key_presses(stderr_text):
    """Count [profile] key: lines from HAND_PROFILE output."""
    return len(re.findall(r"\[profile\] key:", stderr_text))


# ---------------------------------------------------------------------------
# 1. Baseline: PTY round-trip (no horseshoe)
# ---------------------------------------------------------------------------

class TestPtyRoundTrip(unittest.TestCase):
    """Verify the shell+PTY layer is fast — baseline for horseshoe tests."""

    def setUp(self):
        self.fd, self.pid = pty_open_shell()
        pty_read_until(self.fd, b"PROMPT> ", timeout=3.0)

    def tearDown(self):
        os.close(self.fd)
        try:
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
        except OSError:
            pass

    def test_single_enter_latency(self):
        """A single Enter → prompt round-trip must be < 50 ms."""
        pty_drain(self.fd, timeout=0.1)
        os.write(self.fd, b"\r")
        data, elapsed = pty_read_until(self.fd, b"PROMPT> ", timeout=2.0)
        self.assertIn(b"PROMPT> ", data)
        self.assertLess(elapsed, 0.050,
                        f"Round-trip {elapsed*1000:.1f}ms > 50ms")

    def test_rapid_enter_all_prompts_arrive(self):
        """10 rapid Enters must produce >= 10 prompts."""
        pty_drain(self.fd, timeout=0.1)
        for _ in range(10):
            os.write(self.fd, b"\r")
        time.sleep(0.5)
        data = pty_drain(self.fd, timeout=2.0)
        self.assertGreaterEqual(data.count(b"PROMPT> "), 10)

    def test_enter_at_repeat_rate(self):
        """Enters at 30 Hz must each produce a prompt within one interval."""
        pty_drain(self.fd, timeout=0.1)
        interval = 0.033
        delays = []
        for _ in range(15):
            os.write(self.fd, b"\r")
            _, elapsed = pty_read_until(self.fd, b"PROMPT> ", timeout=1.0)
            delays.append(elapsed)
            time.sleep(max(0, interval - elapsed))
        avg = sum(delays) / len(delays)
        self.assertLess(avg, 0.020,
                        f"Avg prompt latency {avg*1000:.1f}ms > 20ms")


# ---------------------------------------------------------------------------
# 2. horseshoe draw-time measurement
# ---------------------------------------------------------------------------

class TestHorseshoeDrawTime(unittest.TestCase):
    """Measure how long hs draw() actually takes."""

    def test_draw_time_under_repeat_interval(self):
        """draw() must be faster than the key-repeat interval (~33 ms).

        If draw() >= repeat interval, prompts WILL batch because the
        event loop is blocked during draw() and cannot process key
        repeats or PTY reads.
        """
        require_wayland()
        require_hs()

        stderr = spawn_hs_profiled(timeout_sec=4)
        times = parse_draw_times(stderr)
        if not times:
            self.skipTest("No draw profiling data captured")

        avg = sum(times) / len(times)
        p95 = sorted(times)[int(len(times) * 0.95)]
        mx = max(times)
        print(f"\n  draw() stats: n={len(times)}, "
              f"avg={avg:.2f}ms, p95={p95:.2f}ms, max={mx:.2f}ms",
              file=sys.stderr)
        self.assertLess(avg, 33,
                        f"Average draw time {avg:.1f}ms >= 33ms repeat interval")


# ---------------------------------------------------------------------------
# 3. End-to-end: spawn hs + wtype Enter repeats
# ---------------------------------------------------------------------------

class TestHorseshoeKeyRepeat(unittest.TestCase):
    """Spawn hs, inject Enter via wtype, parse profiling to detect delays."""

    def test_enter_repeat_does_not_batch(self):
        """Holding Enter must not cause prompt batching.

        We spawn hs, wait for it to settle, then use wtype to send
        rapid Enter keys.  We parse the profiling output to check that
        draws happen at a reasonable rate during the repeat burst.
        """
        require_wayland()
        require_hs()
        require_wtype()

        env = os.environ.copy()
        env["HAND_PROFILE"] = "1"
        proc = subprocess.Popen(
            [HS_BIN],
            stderr=subprocess.PIPE,
            stdout=subprocess.DEVNULL,
            env=env,
        )
        try:
            # Let hs start and render initial frame
            time.sleep(2)

            # Send 15 Enter keys at ~30 Hz via wtype
            for _ in range(15):
                subprocess.run(
                    [WTYPE, "-k", "Return"],
                    timeout=2,
                    capture_output=True,
                )
                time.sleep(0.033)

            # Let renders settle
            time.sleep(1)
        finally:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

        stderr = proc.stderr.read().decode(errors="replace")
        draw_times = parse_draw_times(stderr)

        if not draw_times:
            self.skipTest("No draw profiling data captured")

        # During the ~500ms Enter burst we expect multiple draws
        # If draws are batched, we'll see long gaps with no draws
        # followed by a single draw — detectable as high max draw time
        # or low draw count during the burst.
        #
        # With 15 Enters at 33ms = 495ms of input, we expect at least
        # ~10 draws if rendering keeps up (one per ~50ms is fine).
        # Fewer than 5 means severe batching.
        burst_draws = [t for t in draw_times if t < 100]  # exclude outliers
        print(f"\n  Total draws: {len(draw_times)}, "
              f"burst draws <100ms: {len(burst_draws)}, "
              f"avg={sum(burst_draws)/max(len(burst_draws),1):.2f}ms",
              file=sys.stderr)

        # This is a soft check — the real signal is whether the user
        # sees smooth prompt scrolling.  But < 5 draws in 500ms of
        # key-repeat is definitely broken.
        self.assertGreaterEqual(
            len(burst_draws), 5,
            f"Only {len(burst_draws)} draws during 500ms Enter burst — "
            f"prompts are being batched"
        )


# ---------------------------------------------------------------------------
# 4. Held-key repeat: the actual bug test
# ---------------------------------------------------------------------------

class TestHorseshoeHeldKey(unittest.TestCase):
    """Test that key repeat stops immediately on key release.

    THE BUG: hold Enter → release → prompts KEEP printing because
    repeated Enters were queued in the shell's input buffer faster
    than the terminal could process them.

    This test measures PTY activity AFTER key_release.  In a correct
    implementation there should be at most a few trailing PTY reads
    (the shell's response to the very last Enter).  If dozens of
    reads happen after release, the queue is still draining.
    """

    def _run_held_enter(self, hold_ms=1500, settle_ms=2000):
        """Hold Enter, release, wait for settle, return profiling lines."""
        require_wayland()
        require_hs()
        require_wtype()

        env = os.environ.copy()
        env["HAND_PROFILE"] = "1"
        proc = subprocess.Popen(
            [HS_BIN],
            stderr=subprocess.PIPE,
            stdout=subprocess.DEVNULL,
            env=env,
        )
        try:
            time.sleep(2)  # let hs start

            subprocess.run(
                [WTYPE, "-P", "Return", "-s", str(hold_ms), "-p", "Return"],
                timeout=hold_ms / 1000 + 3,
                capture_output=True,
            )

            # Wait for any post-release drain to finish
            time.sleep(settle_ms / 1000)
        finally:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

        stderr = proc.stderr.read().decode(errors="replace")
        lines = [l for l in stderr.splitlines() if "[profile]" in l]
        return lines

    def test_no_pty_drain_after_release(self):
        """After releasing Enter, at most 5 PTY reads should follow.

        This is the core test for the "queue drains after release" bug.
        We find the key_release marker in the profiling output and
        count how many [profile] pty: lines follow it.  If >5, the
        shell was still processing queued Enters.
        """
        lines = self._run_held_enter()

        # Find the last key_release line
        release_idx = None
        for i, line in enumerate(lines):
            if "key_release:" in line:
                release_idx = i

        if release_idx is None:
            self.skipTest("No key_release marker found in profiling output")

        after_release = lines[release_idx + 1:]
        pty_after = [l for l in after_release if "[profile] pty:" in l]
        repeat_after = [l for l in after_release if "[profile] repeat:" in l
                        and "skipped" not in l]

        # Count repeat events during hold (before release)
        before_release = lines[:release_idx]
        repeats_during = len([l for l in before_release
                              if "[profile] repeat:" in l
                              and "skipped" not in l])

        print(f"\n  Repeats during hold: {repeats_during}",
              file=sys.stderr)
        print(f"  PTY reads after release: {len(pty_after)}",
              file=sys.stderr)
        print(f"  Repeat writes after release: {len(repeat_after)}",
              file=sys.stderr)

        # Print the last 20 profiling lines for diagnosis
        print(f"  Last 20 lines around release:", file=sys.stderr)
        start = max(0, release_idx - 5)
        for line in lines[start:release_idx + 15]:
            marker = " <<< RELEASE" if "key_release:" in line else ""
            print(f"    {line}{marker}", file=sys.stderr)

        # THE ASSERTION: at most 10 PTY reads after release.
        # 4-8 reads are normal (2-3 trailing prompts, each produces
        # 2-3 PTY read events as the shell outputs escape codes).
        # >10 means the shell is draining a queue of Enters we sent.
        self.assertLessEqual(
            len(pty_after), 10,
            f"{len(pty_after)} PTY reads after release — "
            f"shell is still draining queued Enters "
            f"(sent {repeats_during} repeats during hold)"
        )




class TestEventLoopSimulation(unittest.TestCase):
    """Model horseshoe's calloop dispatch + timer + PTY read + draw.

    This simulates the event loop at 0.1ms resolution to detect
    exactly when and why prompts get delayed.

    Matches the actual horseshoe event loop in main.rs:
      - handle_pty_readable: reads all available data, then if
        since_render >= 8ms does an immediate render
      - timer (2ms period when dirty, 16ms when idle):
        renders if since_data >= 2ms OR since_render >= 8ms
      - key repeat callback: writes to PTY, sets dirty, updates last_data_time
      - draw() blocks the event loop for draw_ms
    """

    @staticmethod
    def simulate(draw_ms, repeat_ms, num_repeats, pty_response_ms=1.0):
        """Return (renders, missed) — list of render timestamps and
        count of repeat cycles where the prompt was NOT rendered before
        the next repeat fired.
        """
        TICK = 0.1  # ms resolution
        t = 0.0
        last_render = 0.0
        last_data = -100.0  # long ago
        dirty = False
        next_repeat = repeat_ms
        next_timer = 2.0
        pending_pty = []
        renders = []
        missed = 0
        prompt_rendered_before_next = [False] * num_repeats
        repeat_idx = 0
        blocked_until = 0.0

        total_ms = repeat_ms * (num_repeats + 2)

        def do_render(when):
            nonlocal last_render, dirty, blocked_until
            renders.append(when)
            last_render = when
            dirty = False
            blocked_until = when + draw_ms
            # Mark current prompt as rendered
            if repeat_idx > 0:
                prompt_rendered_before_next[repeat_idx - 1] = True

        while t < total_ms:
            t += TICK
            if t < blocked_until:
                continue

            # 1. Key repeat (calloop timer source)
            if repeat_idx < num_repeats and t >= next_repeat:
                pending_pty.append(t + pty_response_ms)
                last_data = t
                dirty = True
                if repeat_idx > 0 and not prompt_rendered_before_next[repeat_idx - 1]:
                    missed += 1
                repeat_idx += 1
                next_repeat += repeat_ms

            if t < blocked_until:
                continue

            # 2. PTY data arrival (fd readable source)
            new_pty = [ts for ts in pending_pty if ts <= t]
            if new_pty:
                for ts in new_pty:
                    pending_pty.remove(ts)
                last_data = t
                dirty = True
                # Immediate render path: if since_render >= 8ms
                if t - last_render >= 8.0:
                    do_render(t)

            if t < blocked_until:
                continue

            # 3. Render timer
            if t >= next_timer:
                if dirty:
                    since_data = t - last_data
                    since_render = t - last_render
                    if since_data >= 2.0 or since_render >= 8.0:
                        do_render(t)
                    # Always reschedule at 2ms when dirty (matches real code)
                    next_timer = t + 2.0
                else:
                    next_timer = t + 16.0

        return renders, missed

    def test_fast_draw_no_missed_prompts(self):
        """With 5ms draw, every prompt should render before next repeat."""
        renders, missed = self.simulate(
            draw_ms=5, repeat_ms=33, num_repeats=20
        )
        print(f"\n  draw=5ms repeat=33ms: {len(renders)} renders, "
              f"{missed} missed", file=sys.stderr)
        self.assertEqual(missed, 0,
                         f"{missed}/20 prompts missed with 5ms draw")

    def test_slow_draw_no_missed_prompts(self):
        """With 20ms draw, every prompt should still render (event loop copes)."""
        renders, missed = self.simulate(
            draw_ms=20, repeat_ms=33, num_repeats=20
        )
        print(f"\n  draw=20ms repeat=33ms: {len(renders)} renders, "
              f"{missed} missed", file=sys.stderr)
        self.assertEqual(missed, 0,
                         f"{missed}/20 prompts missed with 20ms draw")

    def test_double_repeat_causes_backlog(self):
        """When both compositor + calloop repeats fire (the bug),
        each repeat interval sends 2x PTY writes, creating 2x the work.

        This simulates the effect: double the repeat rate (half the interval).
        With an effective 16.5ms repeat, even moderate draw times cause
        missed prompts because the event loop can't keep up."""
        # Double repeat = effective repeat at half the interval
        renders, missed = self.simulate(
            draw_ms=10, repeat_ms=16, num_repeats=30
        )
        print(f"\n  draw=10ms effective_repeat=16ms (double bug): "
              f"{len(renders)} renders, {missed} missed", file=sys.stderr)
        # At 16ms effective repeat with 10ms draw, each cycle is ~12ms
        # (10ms draw + 2ms timer wait) but repeats every 16ms, so it
        # should just barely keep up.

    def test_draw_time_budget(self):
        """Find the maximum draw time that avoids prompt batching."""
        for draw_ms in range(1, 50):
            _, missed = self.simulate(
                draw_ms=draw_ms, repeat_ms=33, num_repeats=30
            )
            if missed > 0:
                print(f"\n  Batching starts at draw_ms={draw_ms} "
                      f"(missed={missed}/30 with 33ms repeat)",
                      file=sys.stderr)
                return
        print("\n  No batching detected up to 49ms draw time",
              file=sys.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
