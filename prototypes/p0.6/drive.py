# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Drive p06-panes inside a real pty and record what the app rendered.

usage: drive.py <sandbox> <out-dir> <run>     run is "a" (keys, resize, detach, overflow) or "b" (nudge)

The app dumps its ratatui Buffer after every draw (--dump). This script types keys into the pty,
resizes the pty with TIOCSWINSZ, waits for the dumped screen to match, and writes each step.
"""
import fcntl
import os
import pty
import re
import signal
import struct
import sys
import termios
import threading
import time

GATEWAY, WORKSPACE = "jackin-local", "default"
sandbox, out, run = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(out, exist_ok=True)
dump_dir = os.path.join(out, f"dump-{run}")
os.makedirs(dump_dir, exist_ok=True)
log_path = os.path.join(out, f"app-{run}.log")
report = open(os.path.join(out, f"run-{run}.txt"), "w")
checks = []


def say(text=""):
    print(text, flush=True)
    report.write(text + "\n")
    report.flush()


def winsize(cols, rows):
    return struct.pack("HHHH", rows, cols, 0, 0)


def spawn(cols, rows, extra):
    pid, fd = pty.fork()
    if pid == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, winsize(cols, rows))
        os.environ["TERM"] = "xterm-256color"
        os.environ.pop("NO_COLOR", None)
        os.execv("./target/debug/p06-panes", ["p06-panes", GATEWAY, WORKSPACE, sandbox,
                                                "--dump", dump_dir, "--log", log_path, *extra])
    raw = open(os.path.join(out, f"raw-{run}.bin"), "wb")

    def drain():
        while True:
            try:
                data = os.read(fd, 65536)
            except OSError:
                break
            if not data:
                break
            raw.write(data)
            raw.flush()

    threading.Thread(target=drain, daemon=True).start()
    return pid, fd


def screen():
    try:
        return open(os.path.join(dump_dir, "screen.txt")).read()
    except FileNotFoundError:
        return ""


def ansi_rows():
    try:
        return open(os.path.join(dump_dir, "screen.ansi")).read().split("\n")
    except FileNotFoundError:
        return []


CELL = re.compile(r"\x1b\[0;(39|38;5;\d+|38;2;\d+;\d+;\d+);(49|48;5;\d+|48;2;\d+;\d+;\d+)(;1)?m(.)")


def cell(x, y):
    rows = ansi_rows()
    if y >= len(rows):
        return None
    cells = CELL.findall(rows[y])
    return cells[x] if x < len(cells) else None


def log_lines():
    try:
        return open(log_path).read().splitlines()
    except FileNotFoundError:
        return []


def wait_for(predicate, timeout=20, what=""):
    end = time.time() + timeout
    while time.time() < end:
        text = screen()
        if predicate(text):
            return text
        time.sleep(0.05)
    say(f"TIMEOUT waiting for: {what}")
    say(screen())
    return None


def check(name, ok):
    checks.append(ok)
    say(f"CHECK {name}: {'PASS' if ok else 'FAIL'}")


def snap(title, rows=None):
    text = screen()
    lines = text.rstrip("\n").split("\n")
    if rows:
        lines = lines[rows[0]:rows[1]]
    say(f"--- screen: {title} ---")
    for line in lines:
        say("|" + line)
    say("--- end ---")
    return text


class Term:
    def __init__(self, cols, rows, extra=()):
        self.cols, self.rows = cols, rows
        self.pid, self.fd = spawn(cols, rows, list(extra))

    def send(self, data):
        os.write(self.fd, data)

    def type(self, text):
        for ch in text:
            self.send(ch.encode())
            time.sleep(0.03)

    def resize(self, cols, rows):
        self.cols, self.rows = cols, rows
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, winsize(cols, rows))

    def alive(self):
        done, _ = os.waitpid(self.pid, os.WNOHANG)
        return done == 0

    def quit(self):
        self.send(b"\x11")
        for _ in range(100):
            if not self.alive():
                return True
            time.sleep(0.1)
        os.kill(self.pid, signal.SIGKILL)
        return False


def row_of(text, needle):
    for index, line in enumerate(text.split("\n")):
        if needle in line:
            return index
    return None


def tick_of(text):
    match = re.search(r"tick (\d+)", text)
    return int(match.group(1)) if match else None


def run_a():
    say(f"RUN a: sandbox={sandbox} terminal 120x30 (pane inner 58x27)")
    t = Term(120, 30)
    text = wait_for(lambda s: "main pid=" in s and "tick " in s and "$" in s, 30, "both panes live")
    snap("initial: left main process, right shell")
    check("both panes attach and render (left main banner, right bash prompt)", text is not None)

    say("\n## Proof 1: keys reach the focused pane")
    t.type("hello")
    text = wait_for(lambda s: "typed: hello" in s, 10, "left shows typed: hello")
    snap("after typing 'hello' with focus on the left pane", (0, 8))
    check("1a keys typed into the focused left pane reached the main process (typed: hello)", text is not None)
    right_before = screen()
    check("1b the unfocused right pane did not receive them", "hello" not in "\n".join(l[60:] for l in right_before.split("\n")))
    t.send(b"\x17")
    wait_for(lambda s: "focus (right)" in s, 5, "focus right")
    t.type("stty size\r")
    text = wait_for(lambda s: re.search(r"^.{60}.*\b27 58\b", s, re.M) is not None, 10, "stty size 27 58 in right pane")
    snap("Ctrl-W then 'stty size' in the right pane", (0, 12))
    check("1c Ctrl-W moved focus; keys typed into the right pane reached its shell (stty size = 27 58)", text is not None)
    check("1d the left pane did not receive the right pane's keys", "stty" not in "\n".join(l[:60] for l in screen().split("\n")))

    say("\n## Proof 2: terminal resize reaches the remote with the pane's inner size")
    t.resize(140, 36)
    text = wait_for(lambda s: "pty=33x68" in s, 10, "left main process sees pty=33x68")
    t.type("stty size\r")
    text2 = wait_for(lambda s: re.search(r"\b33 68\b", s) is not None, 10, "stty size 33 68 in right pane")
    snap("after the terminal grew to 140x36 (pane inner 68x33)", (0, 14))
    lines = log_lines()
    check("2a left main process saw SIGWINCH and the new PTY size (pty=33x68 in its header row)", text is not None)
    check("2b right shell reports stty size = 33 68", text2 is not None)
    check("2c app sent window_change/Resize cols=68 rows=33 for both panes",
          sum("window_change/Resize cols=68 rows=33 (sent)" in l for l in lines) == 2)
    t.resize(6, 3)
    time.sleep(1.0)
    check("2d a 6x3 terminal does not panic (app still running)", t.alive())
    check("2d2 the tiny size is not forwarded to the remote (floor 20x5)",
          any("below floor" in l for l in log_lines()) and not any("cols=1 rows=1" in l for l in log_lines()))
    t.resize(140, 36)
    text = wait_for(lambda s: "pty=33x68" in s and "main pid=" in s, 15, "recovered after tiny terminal")
    snap("back at 140x36 after the 6x3 terminal", (0, 6))
    check("2e after the tiny terminal the panes recover to 68x33", text is not None)

    say("\n## Proof 3: detach, reattach, replay")
    t.send(b"\x17")
    wait_for(lambda s: "focus (left)" in s, 5, "focus left")
    t.type("abc")
    before = wait_for(lambda s: "typed: helloabc" in s, 10, "typed helloabc")
    tick_before = tick_of(before or "")
    header_before = [l[:70] for l in (before or "").split("\n")[1:5]]
    t.send(b"\x18")
    detached = wait_for(lambda s: "left main [detached" in s, 10, "left detached")
    snap("after Ctrl-X on the left pane (channel closed; the last screen stays)", (0, 8))
    tick_detached = tick_of(detached or "")
    time.sleep(5)
    check("3a detach left the main process running (still detached, no exit)", "ended" not in screen().split("\n")[0])
    t.send(b"\x18")
    after = wait_for(lambda s: "attached #2" in s and "typed: helloabc" in s, 15, "reattach replay")
    snap("right after Ctrl-X reattach (fresh parser fed by the replay)", (0, 8))
    text = wait_for(lambda s: (tick_of(s) or 0) >= (tick_detached or 0) + 8, 15, "live ticks continue")
    snap("live ticks continue after the replay", (0, 8))
    header_after = [l[:70] for l in (text or "").split("\n")[1:5]]
    check("3b replay re-rendered the header rows 1-3 that the main process painted once before the detach (it repaints them only on SIGWINCH)",
          after is not None and "main pid=" in after and "-----" in after)
    check("3c replay kept the typed state 'helloabc' typed before the detach", after is not None)
    check("3d replayed screen equals the pre-detach screen on the unchanged rows (header, typed)",
          header_before[:3] == header_after[:3])
    check("3e tick counter continued past the detached period (ticks while detached were replayed or live)",
          text is not None and (tick_of(text) or 0) > (tick_before or 0) + 4)
    first = [l for l in log_lines() if l.startswith("[") and "left first chunk" in l]
    say("left first chunk lines:\n  " + "\n  ".join(first))

    overflow(t, "plain (no resync, no nudge)", resync=False, nudge=False)
    ok = t.quit()
    raw = open(os.path.join(out, f"raw-{run}.bin"), "rb").read()
    check("4 Ctrl-Q quits and the terminal is restored (alt screen left, cursor shown)",
          ok and b"\x1b[?1049l" in raw and b"\x1b[?25h" in raw)


MID_CSI = re.compile(r"^(\[[0-9;]*|[0-9;]+)[A-Za-z]")


def overflow(t, label, resync, nudge):
    """Flood until the replay window starts inside a CSI sequence, then look at the pane."""
    say(f"\n## Proof 3b ({label}): replay when the 1 MiB window starts mid escape sequence")
    heads = []
    attach_no = int(re.search(r"attached #(\d+)", screen()).group(1)) if re.search(r"attached #(\d+)", screen()) else 1
    for attempt in range(1, 21):
        t.type("flood\r")
        done = wait_for(lambda s: "flood done f=1500" in s, 90, "flood done")
        check(f"3f flood {attempt} completed while attached", done is not None)
        if attempt == 1:
            snap("after the flood (several MB of in-place repaints) while attached", (0, 10))
            check_colors(screen(), "live pane after flood")
        t.send(b"\x18")
        wait_for(lambda s: "[detached" in s, 10, "detach")
        time.sleep(1.5)
        t.send(b"\x18")
        attach_no += 1
        wait_for(lambda s: f"attached #{attach_no}" in s, 30, f"attached #{attach_no}")
        time.sleep(0.25)
        before_nudge = screen()
        chunk = [l for l in log_lines() if "left first chunk" in l][-1]
        head = chunk.split("head=")[1]
        heads.append(head)
        say(f"attempt {attempt}: replay head = {head}")
        if MID_CSI.match(head):
            break
        time.sleep(0.5 if nudge else 0)
    mid = bool(MID_CSI.match(heads[-1]))
    check(f"3g the replay window starts mid escape sequence (head {heads[-1][:12]!r}) after {len(heads)} floods", mid)
    snap(f"replay after overflow, fresh parser ({label}), 0.25 s after attach", (0, 14))
    replay = before_nudge
    check("3h replay after overflow shows the flood log rows", "log f=1500" in replay)
    check_colors(replay, f"replay after overflow, {label}")
    row1 = replay.split("\n")[1][1:69].strip()
    if resync:
        say([l for l in log_lines() if "resync:" in l][-1])
        check("3i resync: no fragment of the cut sequence is painted (row 1 empty)", row1 == "")
    elif nudge:
        check("3i before the nudge the cut fragment is on row 1 (the fresh parser painted the partial sequence as text)", row1 != "" and "main pid=" not in row1)
        time.sleep(1.5)
        text = screen()
        snap("after the nudge: window_change rows-1 then rows -> SIGWINCH -> the app repaints", (0, 14))
        check("3j nudge: header rows and typed row restored, fragment gone", text.split("\n")[1][1:69].strip().startswith("main pid=") and "-----" in text)
    else:
        say(f"INFO row 1 of the pane = {row1!r}")
        check("3i plain: the cut fragment is painted as text on row 1 and the header rows painted before the window are lost",
              row1 != "" and "main pid=" not in replay)


def check_colors(text, label):
    bad = []
    for r in range(6, 12):
        got = cell(1, r)
        want = f"38;5;{r % 7 + 1}"
        if got is None or got[0] != want or got[2] != ";1":
            bad.append((r, got, want))
    check(f"colors survive the parser and the widget ({label}): row r is bold and indexed color r%7+1", not bad)
    if bad:
        say(f"  mismatches: {bad[:3]}")


def run_variant(flags, label, resync, nudge):
    say(f"RUN {run}: sandbox={sandbox} terminal 140x36 flags={flags}")
    t = Term(140, 36, flags)
    wait_for(lambda s: "attached #1" in s and "tick " in s, 30, "replay visible")
    overflow(t, label, resync, nudge)
    t.quit()


if run == "a":
    run_a()
elif run == "b":
    run_variant(["--resync"], "resync", True, False)
else:
    run_variant(["--nudge"], "nudge", False, True)
say(f"\nSUMMARY {sum(checks)}/{len(checks)} checks passed")
