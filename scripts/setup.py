#!/usr/bin/env python3
"""Setup on Windows and Linux (install.sh does it on the Mac): the helper scripts, DepotDownloader and the chosen
games' files from the player's Steam account, each chosen game's mod, config.json. Experimental and untested.

The window runs this as install.sh (DROPPOD_GUI=1) and reads the same output: "==> <step>" headings, and
"status: ", "progress: ", "qr..." and "error: " lines. Takes install.sh's variables: DROPPOD_HOME, DROPPOD_GAMES,
DROPPOD_GAME_FILES, DROPPOD_SOURCE, DROPPOD_GUI, DROPPOD_UPDATE, DROPPOD_REPAIR, DROPPOD_DRY_RUN, DROPPOD_CACHE.
DROPPOD_PLATFORM=windows|linux does a dry run for that system from any other.

Nothing Mac-only: no Wine, DXVK, Mesa, spinfix, loopback addresses, game apps or tuned graphics settings. On Linux
the Windows mods need the system's Wine (play.py says so); on Windows they run as they are.
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent  # the scripts folder beside the app
sys.path.insert(0, str(HERE))
import games  # noqa: E402

NAME = games.NAME
BASE = games.BASE
CACHE = Path(os.environ.get("DROPPOD_CACHE") or BASE / "downloads")
CONFIG = BASE / "config.json"
GUI, UPDATE, REPAIR, DRY = (os.environ.get(f"DROPPOD_{v}", "") for v in ("GUI", "UPDATE", "REPAIR", "DRY_RUN"))
# name, url, sha256 (checked 2026-10-23 against the release's downloads)
DEPOT_PKG = {
    "windows": ("DepotDownloader-windows-x64.zip", "41c9e9f0df54b3ad02e67a11726756e5c73283bd7c2e1b04acfa5ae4c2ed3767"),
    "linux": ("DepotDownloader-linux-x64.zip", "a999dec66b4850fc961bd50366696d23c2d0fad7b18790e6a5647b2f19097a53"),
}
DEPOT_URL = "https://github.com/SteamRE/DepotDownloader/releases/download/DepotDownloader_3.4.0/"
DEPOT_EXE = BASE / "tools" / ("DepotDownloader.exe" if games.PLATFORM == "windows" else "DepotDownloader")
SCRIPTS = ["mods.json", "games.py", "mods.py", "steam.py", "updater.py", "tools.py", "xiso.py", "play.py"]
RUNTIME_GB = 1  # the mods and the downloads while they unpack

GAMES = set((os.environ.get("DROPPOD_GAMES") or "halo3").split())
MODS = games.mods_for(GAMES)
config = json.loads(CONFIG.read_text()) if CONFIG.is_file() else {}
GAME_FILES = Path(os.environ.get("DROPPOD_GAME_FILES") or config.get("gameFiles") or BASE / "Game Files")
SOURCE = os.environ.get("DROPPOD_SOURCE") or config.get("source") or "steam"
MISSING = []
step = "starting"


def bold(text):
    global step
    step = text
    print(f"\n\033[1m==> {text}\033[0m", flush=True)


def note(text):
    print(f"    {text}", flush=True)


def dry(text):
    note(f"dry-run: {text}")


class Stop(Exception):
    pass


def die(text):
    raise Stop(text)


def run(script, *args):
    """One of the helper scripts in $BASE/scripts, its output passed on (error output to the log); its last line."""
    cmd = [sys.executable, "-I", "-B", str(BASE / "scripts" / script), *args]
    env = dict(os.environ, DROPPOD_HOME=str(BASE), PYTHONIOENCODING="utf-8")
    last = ""
    with open(BASE / "logs" / "launcher.log", "a", encoding="utf-8") as log:
        p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=log, stdin=subprocess.DEVNULL, text=True,
                             encoding="utf-8", env=env)
        for line in p.stdout:
            line = line.rstrip("\n")
            if line.startswith(("progress:", "status:", "qr")):
                if GUI:
                    print(line, flush=True)
            else:
                note(line)
            last = line
    return p.wait(), last


def gib_free(path):
    return shutil.disk_usage(path).free / 2**30


def step_start():
    global MISSING
    version = (HERE.parent / "VERSION").read_text().strip() if (HERE.parent / "VERSION").is_file() else "dev"
    bold(f"Starting setup: {NAME} {version}")
    note(f"Installs into: {BASE}")
    where = "an existing MCC folder, used in place" if SOURCE == "existing" else "downloaded from Steam"
    note(f"Game files: {GAME_FILES} ({where})")
    note(f"Games: {' '.join(sorted(GAMES))} (mods: {' '.join(MODS)})")
    if DRY:
        note(f"Dry run ({games.PLATFORM}): nothing is downloaded or changed")
    if SOURCE not in ("steam", "existing"):
        die(f"Unknown game files source: {SOURCE}")
    if SOURCE == "existing":
        if not GAME_FILES.is_dir():
            die(f"The MCC folder {GAME_FILES} isn't there. Is its drive connected?")
    elif not GAME_FILES.is_dir() or not any(GAME_FILES.iterdir()):
        # a new, empty folder of our own: uninstall may offer to delete it (never a folder the player had)
        (BASE / ".installed").mkdir(parents=True, exist_ok=True)
        GAME_FILES.mkdir(parents=True, exist_ok=True)
        (BASE / ".installed" / "game-files-owner").write_text(f"{GAME_FILES}\n")
    for d in ("tools", "scripts", "logs", "cache"):
        (BASE / d).mkdir(parents=True, exist_ok=True)
    CACHE.mkdir(parents=True, exist_ok=True)
    GAME_FILES.mkdir(parents=True, exist_ok=True)
    MISSING = [] if UPDATE else games.missing(GAME_FILES, GAMES)
    need = sum(games.DEPOTS[d][1] for d in MISSING)
    home_free, files_free = gib_free(BASE), gib_free(GAME_FILES)
    note(f"Free: {home_free:.0f} GB here, {files_free:.0f} GB for the game files (needs {need:.0f} GB)")
    if home_free < RUNTIME_GB:
        die(f"Need about {RUNTIME_GB} GB free on this disk (have {home_free:.0f} GB).")
    if files_free + 0.5 < need:
        die(f"Need about {need:.0f} GB free for the game files (have {files_free:.0f} GB). "
            "Free up space or choose another folder.")
    if not (BASE / "version").exists() and not DRY:
        (BASE / ".setup-incomplete").touch()


def fetch(name, url, sha):
    """The verified download in $CACHE (a dry run only says so)."""
    f = CACHE / name

    def ok():
        return f.is_file() and hashlib.sha256(f.read_bytes()).hexdigest() == sha
    if DRY and not ok():
        dry(f"download {name} from {url} (sha256 {sha})")
        return f
    if not ok():
        note(f"downloading {name}")
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "droppod"}), timeout=60) as r, \
                    open(f.with_suffix(".part"), "wb") as out:
                shutil.copyfileobj(r, out)
        except OSError:
            die(f"Couldn't download {name}. Check your internet connection and try again.")
        f.with_suffix(".part").replace(f)
    if not ok():
        f.unlink()
        die(f"The download of {name} was damaged. Press Try Again to download it again.")
    return f


def step_depotdownloader():
    if SOURCE != "steam" or not games.needed(GAMES):
        return
    bold("DepotDownloader 3.4.0 (downloads your game files from your Steam account)")
    if not DEPOT_EXE.is_file() or REPAIR:
        name, sha = DEPOT_PKG[games.PLATFORM]
        pkg = fetch(name, DEPOT_URL + name, sha)
        if DRY:
            dry(f"unzip {name} into {BASE / 'tools'}")
        else:
            with zipfile.ZipFile(pkg) as z:
                z.extractall(BASE / "tools")
            DEPOT_EXE.chmod(0o755)
    note("ok")


def step_scripts():
    bold("Helper scripts")
    for f in SCRIPTS:
        shutil.copy2(HERE / f, BASE / "scripts" / f)
    app_name = HERE.parent / "APP_NAME"
    (BASE / "scripts" / "APP_NAME").write_text(app_name.read_text() if app_name.is_file() else f"{NAME}\n")
    note("ok")


def step_mods():
    for mod in MODS:
        m = games.MODS[mod]
        bold(f"Mod: {m['name']} (latest release, checksum-verified)")
        _, last = run("mods.py", "get", mod)
        if last.startswith("error: "):
            die(last[7:])
        if last not in ("ok", "ok already"):
            die(f"Could not download {m['name']}. Check your internet connection and press Try Again.")
        if m["runs"] == "wine" and games.PLATFORM == "linux" and not shutil.which("wine"):
            note(f"{m['name']} is a Windows program: install Wine (your distribution's wine package) to play it")


def step_game_files():
    if not games.needed(GAMES):
        return
    bold(f"Game files ({'your MCC folder' if SOURCE == 'existing' else 'from your Steam account'})")
    note(f"already there: {' '.join(map(str, games.present(GAME_FILES))) or 'nothing'}")
    if not MISSING:
        note("nothing to download")
        return
    for d in MISSING:
        name, gib, _ = games.DEPOTS[d]
        note(f"to download: {d} {gib} {name}")
    if UPDATE:
        note("skipped while updating")
        return
    if not GUI:
        note("You need to own Halo: The Master Chief Collection on Steam.")
        note("A QR code will appear: open the Steam app on your phone > Steam Guard > Scan a QR code.")
    rc, last = run("steam.py", "download", str(GAME_FILES), *map(str, MISSING))
    if rc != 0:
        die(last[7:] if last.startswith("error: ") else "The Steam download stopped. Press Try Again to resume.")
    if not DRY:
        left = games.missing(GAME_FILES, GAMES)
        if left:
            die(f"Some game files are still missing after the download ({' '.join(map(str, left))}). "
                "Press Try Again to resume.")
    note("ok")


def step_config():
    bold("Settings file (config.json)")
    if DRY:
        dry(f"write {CONFIG}")
        return
    c = games.write_config(CONFIG, GAME_FILES, SOURCE, GAMES)
    note("games: " + (" ".join(c["games"]) or "none yet"))
    note(str(CONFIG))


def step_finish():
    if DRY:
        bold("Done (dry run)")
        return
    version = HERE.parent / "VERSION"
    (BASE / "version").write_text(version.read_text() if version.is_file() else "dev\n")
    (BASE / ".setup-incomplete").unlink(missing_ok=True)
    if not os.environ.get("DROPPOD_CACHE"):  # the downloaded archives aren't needed any more
        for f in CACHE.iterdir():
            if f.is_file():
                f.unlink()
    if UPDATE:
        bold(f"Updated to {(BASE / 'version').read_text().strip()}")
        return
    bold("Done")
    note(f"Open {NAME} and press Play on a game.")


def main():
    sys.stdout.reconfigure(encoding="utf-8")  # (the QR code's block characters, on Windows too)
    if games.PLATFORM == "mac":
        print("error: On the Mac, setup is install.sh.", flush=True)
        return 1
    try:
        for s in (step_start, step_depotdownloader, step_scripts, step_mods, step_game_files, step_config, step_finish):
            s()
    except Stop as e:
        print(f"\n\033[31mError: {e}\033[0m", file=sys.stderr, flush=True)
        if GUI:
            print(f"error: {e}", flush=True)  # (the window shows this line)
        return 1
    except Exception as e:  # anything without its own message still tells the window which step it was
        print(f"error: Something went wrong at the step “{step}” ({e}). Check your internet connection and disk "
              "space, then press Try Again.", flush=True)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
