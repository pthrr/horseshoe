"""System tests for horseshoe clipboard copy & paste.

Tests clipboard round-trip via two paths:
  1. External clipboard (wl-copy) → Ctrl+Shift+V paste into hs
  2. OSC 52 copy from inside hs → wl-paste to read back

Requirements (provided via nix-shell):
    wtype        – Wayland keystroke injector
    wl-clipboard – wl-copy / wl-paste CLI tools

Run:
    task test:system:clipboard
"""

import os
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
WL_COPY = shutil.which("wl-copy")
WL_PASTE = shutil.which("wl-paste")


def require_wayland():
    if not os.environ.get("WAYLAND_DISPLAY"):
        raise unittest.SkipTest("No Wayland session (WAYLAND_DISPLAY not set)")


def require_wtype():
    if not WTYPE:
        raise unittest.SkipTest("wtype not found in PATH")


def require_hs():
    if not os.path.isfile(HS_BIN):
        raise unittest.SkipTest(f"hs binary not found at {HS_BIN}")


def require_wl_clipboard():
    if not WL_COPY or not WL_PASTE:
        raise unittest.SkipTest("wl-copy / wl-paste not found in PATH")


class TestClipboardPaste(unittest.TestCase):
    """Paste from external clipboard into horseshoe via Ctrl+Shift+V."""

    def test_paste_from_external_clipboard(self):
        """wl-copy text → Ctrl+Shift+V in hs → [profile] paste: shows text."""
        require_wayland()
        require_hs()
        require_wtype()
        require_wl_clipboard()

        marker = "HORSESHOE_PASTE_42"

        # Set clipboard externally
        subprocess.run(
            [WL_COPY, marker],
            check=True,
            timeout=5,
        )

        env = os.environ.copy()
        env["HAND_PROFILE"] = "1"
        proc = subprocess.Popen(
            [HS_BIN],
            stderr=subprocess.PIPE,
            stdout=subprocess.DEVNULL,
            env=env,
        )
        try:
            # Wait for hs to start and get focus
            time.sleep(2)

            # Ctrl+Shift+V paste
            subprocess.run(
                [WTYPE, "-M", "shift", "-M", "ctrl", "-k", "v",
                 "-m", "ctrl", "-m", "shift"],
                timeout=5,
                capture_output=True,
            )

            # Wait for paste processing
            time.sleep(0.5)
        finally:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

        stderr = proc.stderr.read().decode(errors="replace")

        # Find [profile] paste: lines
        paste_lines = [
            l for l in stderr.splitlines()
            if "[profile] paste:" in l
        ]
        print(f"\n  paste profile lines: {paste_lines}", file=sys.stderr)
        self.assertTrue(
            any(marker in l for l in paste_lines),
            f"Expected '{marker}' in paste profile output.\n"
            f"  paste lines: {paste_lines}\n"
            f"  full stderr tail: {stderr[-500:]}"
        )


class TestClipboardOSC52(unittest.TestCase):
    """Copy via OSC 52 from inside hs, read back with wl-paste."""

    def test_copy_via_osc52_and_paste_back(self):
        """printf OSC 52 inside hs → wl-paste returns the text."""
        require_wayland()
        require_hs()
        require_wtype()
        require_wl_clipboard()

        marker = "HORSESHOE_COPY_42"
        # base64 of "HORSESHOE_COPY_42" is "SE9SU0VTSE9FX0NPUFlfNDI="
        b64 = "SE9SU0VTSE9FX0NPUFlfNDI="

        env = os.environ.copy()
        env["HAND_PROFILE"] = "1"
        proc = subprocess.Popen(
            [HS_BIN],
            stderr=subprocess.PIPE,
            stdout=subprocess.DEVNULL,
            env=env,
        )
        try:
            # Wait for hs to start and get focus
            time.sleep(2)

            # Type the printf command that emits OSC 52
            cmd = f"printf '\\e]52;c;{b64}\\a'"
            subprocess.run(
                [WTYPE, cmd],
                timeout=5,
                capture_output=True,
            )
            # Press Enter to execute
            time.sleep(0.1)
            subprocess.run(
                [WTYPE, "-k", "Return"],
                timeout=5,
                capture_output=True,
            )

            # Wait for OSC 52 processing
            time.sleep(1)

            # Read clipboard while hs is still alive (it owns the selection)
            result = subprocess.run(
                [WL_PASTE, "--no-newline"],
                capture_output=True,
                timeout=5,
            )
            clipboard_content = result.stdout.decode(errors="replace")
        finally:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

        stderr = proc.stderr.read().decode(errors="replace")

        # Check wl-paste output
        print(f"\n  wl-paste output: {clipboard_content!r}", file=sys.stderr)
        self.assertEqual(
            clipboard_content, marker,
            f"wl-paste returned {clipboard_content!r}, expected {marker!r}"
        )

        # Check profiling marker
        set_lines = [
            l for l in stderr.splitlines()
            if "[profile] clipboard_set:" in l
        ]
        print(f"  clipboard_set profile lines: {set_lines}", file=sys.stderr)
        self.assertTrue(
            any(marker in l for l in set_lines),
            f"Expected '{marker}' in clipboard_set profile output.\n"
            f"  set lines: {set_lines}\n"
            f"  full stderr tail: {stderr[-500:]}"
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
