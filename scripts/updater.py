#!/usr/bin/env python3
"""Keep the app up to date (run by the window when it opens).

  updater.py run    the app itself, when its GitHub repo has a newer release

The mods aren't updated here: they're downloaded once (mods.py get) and update themselves when they start.

The app updates from its own GitHub releases (DROPPOD_REPO, "owner/name"; nothing happens while it is unset, i.e.
until the project is published): the release's <Name>.zip (Drop-Pod.zip) is checked against its SHA256SUMS, swapped in for the app
the player opened (DROPPOD_APP_PATH), and the new copy's install.sh runs in update mode.

Prints one line per event for the window: "status: <text>", "progress: <0-100>", "relaunch" (the app was replaced),
and last "done: up to date|updated", "offline: <reason>" or "error: <reason>". Offline or failed updates never stop
a game from starting.

Adapted from the Project Reclaimer Mac port's updater.py (MIT).
"""
import fcntl
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import games  # noqa: E402  (the app's name and folder)
from mods import Offline, download, fetch  # noqa: E402

NAME = games.NAME
BASE = games.BASE
REPO = os.environ.get("DROPPOD_REPO", "")
APP_PATH = os.environ.get("DROPPOD_APP_PATH", "")
APP_VERSION = os.environ.get("DROPPOD_APP_VERSION", "")
APP_ASSET = NAME.replace(" ", "-") + ".zip"  # (as make_dmg.sh names it)


def say(line):
    print(line, flush=True)


def version_tuple(text):
    return tuple(int(x) for x in re.findall(r"\d+", text or "0")[:3]) or (0,)


# ---------------------------------------------------------------------------------------------------------------
# the app itself

def update_app():
    """Returns "current", "updated" (the app was replaced: relaunch), "unpublished" (no repo yet), or raises."""
    if not REPO:
        return "unpublished"
    listing = json.loads(fetch(f"https://api.github.com/repos/{REPO}/releases/latest",
                               headers={"Accept": "application/vnd.github+json"}).decode())
    latest = str(listing.get("tag_name", "")).lstrip("v")
    if not latest or version_tuple(latest) <= version_tuple(APP_VERSION):
        return "current"
    app = Path(APP_PATH)
    # never the read-only disk image's copy, or a copy macOS runs from a temporary place (App Translocation)
    if app.suffix != ".app" or str(app).startswith("/Volumes/") or "AppTranslocation" in str(app) \
            or not os.access(app.parent, os.W_OK):
        raise RuntimeError(f"version {latest} is out, but this copy of the app can't update itself. "
                           f"Move {NAME} to your Applications folder")
    assets = {a.get("name"): a.get("browser_download_url") for a in listing.get("assets", [])}
    if APP_ASSET not in assets or "SHA256SUMS" not in assets:
        raise RuntimeError(f"the {latest} release has no {APP_ASSET}")
    sums = dict(reversed(line.split()) for line in fetch(assets["SHA256SUMS"]).decode().splitlines()
                if len(line.split()) == 2)
    expected = sums.get(APP_ASSET) or sums.get("*" + APP_ASSET)
    if not expected:
        raise RuntimeError(f"the {latest} release doesn't list a checksum for {APP_ASSET}")
    say(f"status: Updating {NAME} to {latest}…")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        download(assets[APP_ASSET], tmp / APP_ASSET, expected.lower(), NAME)
        subprocess.run(["/usr/bin/ditto", "-x", "-k", str(tmp / APP_ASSET), str(tmp / "new")], check=True)
        new = next((tmp / "new").glob("*.app"))
        # replace, don't overwrite in place: the old app is running, and macOS caches a binary's signature per file
        old = app.with_name(app.name + ".old")
        shutil.rmtree(old, ignore_errors=True)
        app.rename(old)
        subprocess.run(["/usr/bin/ditto", str(new), str(app)], check=True)
        subprocess.run(["/usr/bin/xattr", "-dr", "com.apple.quarantine", str(app)], capture_output=True)
        shutil.rmtree(old, ignore_errors=True)
    # the new copy's install.sh refreshes the scripts and Play buttons (never downloads games in update mode)
    install = app / "Contents" / "Resources" / "install" / "install.sh"
    env = dict(os.environ, DROPPOD_HOME=str(BASE), DROPPOD_UPDATE="1", DROPPOD_UPDATE_LOCKED="1",
               DROPPOD_APP_SOURCE=str(app))
    p = subprocess.run(["/bin/bash", str(install)], env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True)
    if p.returncode != 0:
        tail = [l for l in re.sub(r"\x1b\[[0-9;]*m", "", p.stdout + p.stderr).splitlines() if l.strip()]
        raise RuntimeError(re.sub(r"^Error: ", "", tail[-1].strip() if tail else "the installer failed"))
    return "updated"


def main(argv):
    if argv[1:] != ["run"]:
        print(__doc__)
        return 2
    BASE.mkdir(parents=True, exist_ok=True)
    lock = open(BASE / ".update.lock", "w")  # one update at a time (e.g. the app opened twice)
    if not os.environ.get("DROPPOD_UPDATE_LOCKED"):  # (set when install.sh runs inside an app update)
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            say("status: Waiting for another update to finish…")
            fcntl.flock(lock, fcntl.LOCK_EX)
    say("status: Checking for updates…")
    try:
        result = update_app()
        sys.stderr.write(f"updater: update_app: {result}\n")
    except Offline as e:
        sys.stderr.write(f"updater: update_app: can't connect: {e}\n")
        say(f"offline: {e}")
        return 0
    except Exception as e:  # a failed update never blocks a game
        sys.stderr.write(f"updater: update_app: {e}\n")
        say(f"error: {e}")
        return 1
    if result == "updated":
        say("relaunch")
        say("done: updated")
    else:
        say("done: up to date")
    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))
