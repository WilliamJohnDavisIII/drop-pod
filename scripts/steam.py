#!/usr/bin/env python3
"""Download MCC's game files from the player's own Steam account with DepotDownloader, for the window.

  steam.py download <dir> <depot>...   download (or check and finish) these depots into <dir>, one at a time:
                                       QR codes when a sign-in is needed ("qr-begin", "qr: <row>"..., "qr-end"),
                                       "status: <text>", "progress: <0-100 over all of them>", then "ok" or
                                       "error: <plain reason>"
  steam.py login-check                 "ok <account>", "need-login" or "offline": does the saved Steam login work?

The first depot signs in with a QR code (scanned with the Steam app on a phone) unless a saved login still works;
DepotDownloader then remembers it (-remember-password), so the rest, a later Get and Repair don't ask again.
Each depot runs with -validate, so a stopped download picks up where it stopped. DROPPOD_DRY_RUN=1 prints the
DepotDownloader commands instead of running them.

Adapted from the Project Reclaimer Mac port's workshop_helper.py (MIT): its QR relay and download error messages.
"""
import os
import re
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import games  # noqa: E402  (the depot table, next to this script)

BASE = games.BASE
ACCOUNT_FILE = BASE / "steam-account"
DEPOT = Path(os.environ.get("DROPPOD_DEPOTDOWNLOADER", BASE / "tools" / "DepotDownloader"))
APP_ID = str(games.APP_ID)
DRY = os.environ.get("DROPPOD_DRY_RUN", "")
PROGRESS = re.compile(r"^\s*(\d+(?:\.\d+)?)% ")
LOGIN_NEEDED = re.compile(r"Enter account password|Access token was rejected|InvalidPassword|AccountLogonDenied|"
                          r"TwoFactor|LogOn requires|authentication code", re.I)
OFFLINE = re.compile(r"Could not connect to Steam|Connection to Steam failed|ServiceUnavailable|TryAnotherCM", re.I)
NOT_OWNED = re.compile(r"is not available from this account|No subscription|AccessDenied for (app|depot)", re.I)
QR_START = "Use the Steam Mobile App to sign in with this QR code:"
QR_SUCCESS = re.compile(r"login with -username (\S+) -remember-password")
QR_EXPIRED = "The QR code expired before it was scanned. Press Try Again for a new one."
# Steam ends a QR session after 1-2 minutes; finding the Steam app can take longer, so a new code just appears
QR_ATTEMPTS = int(os.environ.get("DROPPOD_QR_ATTEMPTS", 5))
# a quick test that the saved login works: the shared depot's manifest only
CHECK_DEPOT = str(games.BASE_DEPOT)


def say(line):
    print(line, flush=True)


def account():
    """Steam account name saved at QR sign-in (DepotDownloader keeps the login under exactly this name)."""
    try:
        return ACCOUNT_FILE.read_text().strip() or None
    except OSError:
        return None


def stop_with_parent(proc):
    """Kill proc and exit once the process that started us (the window or install.sh) is gone, e.g. force-quit."""
    parent = os.getppid()

    def watch_parent():
        while os.getppid() == parent:
            time.sleep(1)
        proc.kill()
        os._exit(1)
    threading.Thread(target=watch_parent, daemon=True).start()


def run_relaying(cmd, on_line=None):
    """Run DepotDownloader, relaying each QR code it draws and saving the account name Steam reports at QR
    sign-in. Returns (returncode, account name or None, last output lines, whether a QR code was shown)."""
    if DRY:
        say("dry-run: " + " ".join(cmd))
        return 0, None, [], False
    p = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    stop_with_parent(p)
    signal.signal(signal.SIGTERM, lambda *_: (p.kill(), sys.exit(1)))  # (the window's Stop, or quitting it)
    qr, name, tail, shown = None, None, [], False  # qr: rows of the code being relayed, None outside a code
    try:
        for raw in p.stdout:
            line = raw.rstrip("\n")
            # QRCoder rows: "██" per dark module, two spaces per light one, quiet zone included. The code is square,
            # so it ends after as many rows as it has modules: DepotDownloader then prints nothing until the scan
            if line and set(line) <= {"█", " "}:
                if qr is None:
                    qr, shown = [], True
                    say("qr-begin")
                qr.append(line)
                say("qr: " + line)
                if len(qr) >= len(qr[0]) // 2:
                    say("qr-end")
                    qr = None
                continue
            if qr is not None:  # a shorter code than expected; end it anyway
                say("qr-end")
                qr = None
            if line.strip() and line != QR_START and "QR code has changed" not in line:
                tail = (tail + [line])[-20:]
            m = QR_SUCCESS.search(line)
            if m:
                name = m.group(1)
                ACCOUNT_FILE.parent.mkdir(parents=True, exist_ok=True)
                ACCOUNT_FILE.write_text(name + "\n")
            if on_line:
                on_line(line)
        if qr is not None:
            say("qr-end")
        p.wait(timeout=60)
    finally:
        if p.poll() is None:
            p.kill()
    return p.returncode, name, tail, shown


def qr_expired(rc, shown, tail):
    return rc != 0 and shown and bool(tail) and "InitializeSteam" in tail[-1]


def plain_reason(tail, default):
    reason = tail[-1].strip() if tail else default
    return re.sub(r"^Unhandled exception\. [\w.]+: ", "", reason)


def download_error(tail):
    text = "\n".join(tail)
    if NOT_OWNED.search(text):
        return "error: This Steam account doesn't own Halo: The Master Chief Collection."
    if OFFLINE.search(text):
        return "error: Couldn't reach Steam. Check your internet connection and try again."
    if re.search(r"No space left|disk full", text, re.I):
        return "error: The disk with your game files is full. Free up some space, then press Try Again."
    sys.stderr.write(text + "\n")
    return "error: " + plain_reason(tail, "the download stopped") + " Press Try Again to resume."


def download_one(dest, depot, on_line):
    """One depot: the saved login first, a QR code only when it no longer works. Returns "ok" or "error: ..."."""
    base = [str(DEPOT), "-app", APP_ID, "-depot", str(depot), "-os", "windows", "-dir", str(dest), "-validate"]
    name = account()
    if name:
        rc, _, tail, _ = run_relaying(base + ["-username", name, "-remember-password"], on_line=on_line)
        if rc == 0:
            return "ok"
        if not LOGIN_NEEDED.search("\n".join(tail)):
            return download_error(tail)
    for _ in range(QR_ATTEMPTS):
        rc, _, tail, shown = run_relaying(base + ["-qr", "-remember-password"], on_line=on_line)
        if not qr_expired(rc, shown, tail):
            return "ok" if rc == 0 else download_error(tail)
    return "error: " + QR_EXPIRED


def download(dest, depots):
    Path(dest).mkdir(parents=True, exist_ok=True)
    sizes = {d: games.DEPOTS[d][1] for d in depots}
    total = sum(sizes.values()) or 1
    done_gib = 0.0
    for i, depot in enumerate(depots):
        name = games.DEPOTS[depot][0]
        label = f"{name} ({i + 1} of {len(depots)})" if len(depots) > 1 else name
        say(f"status: Downloading {label}…")
        last, signed_in = [0.0], [account() is not None]

        def progress(line, start=done_gib, size=sizes[depot], label=label):
            if not signed_in[0] and (QR_SUCCESS.search(line) or re.match(r"Got \d+ licenses", line)):
                signed_in[0] = True  # (the window takes the QR code away)
                say(f"status: Signed in to Steam. Downloading {label}…")
            m = PROGRESS.match(line)
            if m and time.time() - last[0] >= 1:
                fraction = min(float(m.group(1)), 100.0) / 100
                say(f"progress: {100 * (start + fraction * size) / total:.1f}")
                last[0] = time.time()

        result = download_one(dest, depot, progress)
        if result != "ok":
            return result
        done_gib += sizes[depot]
        say(f"progress: {100 * done_gib / total:.1f}")
    return "ok"


def login_check():
    name = account()
    if not name:
        return "need-login"
    cmd = [str(DEPOT), "-app", APP_ID, "-depot", CHECK_DEPOT, "-os", "windows", "-manifest-only",
           "-username", name, "-remember-password", "-dir", str(BASE / "logs" / "login-check")]
    if DRY:
        say("dry-run: " + " ".join(cmd))
        return f"ok {name}"
    p = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    stop_with_parent(p)
    # DepotDownloader retries Steam's servers for minutes when it can't reach them; the window shouldn't wait that long
    timer = threading.Timer(float(os.environ.get("DROPPOD_LOGIN_TIMEOUT", 45)), p.kill)
    timer.daemon = True
    timer.start()
    out = []
    try:
        for line in p.stdout:
            out.append(line)
            if re.match(r"Got \d+ licenses", line):
                p.terminate()
                return f"ok {name}"
            if LOGIN_NEEDED.search(line):
                p.kill()
                return "need-login"
        p.wait()
    finally:
        timer.cancel()
        if p.poll() is None:
            p.kill()
    text = "".join(out)
    if OFFLINE.search(text) or p.returncode != 0 and not text.strip():
        return "offline"
    return f"ok {name}" if p.returncode == 0 else "need-login"


def main(argv):
    if len(argv) >= 4 and argv[1] == "download":
        try:
            depots = [int(d) for d in argv[3:]]
            unknown = [d for d in depots if d not in games.DEPOTS]
        except ValueError:
            unknown = argv[3:]
        if unknown:
            say(f"error: unknown depot {unknown[0]}")
            return 2
        result = download(argv[2], depots)
        say(result)
        return 0 if result == "ok" else 1
    if len(argv) == 2 and argv[1] == "login-check":
        result = login_check()
        say(result)
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
