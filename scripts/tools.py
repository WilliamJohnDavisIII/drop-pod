#!/usr/bin/env python3
"""The window's Settings and automatic checks, one command per action.

  net-check          "ok" or "missing": are the local network addresses Halo 3 (Project Reclaimer) needs on lo0?
  net-fix            reinstall + restart the network startup task (one macOS password prompt)
  controller-check   "stuck" if macOS's game controller service is stuck (no app sees controllers until a restart)
  crash-check <game> "crash: <plain explanation>" if that game's last log shows a crash, else "ok"
  force-quit         stop every game, Wine and wineserver of this install (frozen windows Force Quit can't close)
  report             zip the logs and versions (no Steam account name) to the Desktop: "ok <path>"
  reset-graphics     put back Halo 3's tuned graphics settings
  forget-steam       forget the saved Steam account (the next download shows a QR code)
  repair             delete crash dumps, then check the installed games' files against Steam (QR codes and progress
                     as steam.py download)
  crash-reports      "<bytes>": how much the games' crash dumps take (crash_report/ in the game files folder)
  clean-crash-reports   delete them: "ok <bytes freed>"
  set-game-files <dir>   the game files are now in <dir> (moved to another drive): update config.json and the links
  play-with <game> <mod>   what that game's Play button runs (one of its mods in mods.json)
  native-ce-import <image>   copy Halo's maps folder out of the player's Xbox disc image into ChupathingyCE's data
                     folder (what its own first start does), then check them: progress lines, then "ok" or "ok already"
  native-ce-status   "ready", "needs-port" (not downloaded: mods.py get chupathingyce) or "needs-maps" (no Xbox maps
                     imported yet)
  uninstall [delete-game-files]   remove the app, its folder and the network startup task; the game files stay
                     unless asked (and only when this app downloaded them into a folder of its own)

Each prints "ok" (or its result) last, or "error: <plain reason>". DROPPOD_DRY_RUN=1 prints admin commands
("dry-run admin: ...") instead of asking for the password, for tests.

Adapted from the Project Reclaimer Mac port's tools.py (MIT).
"""
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import games  # noqa: E402
import mods  # noqa: E402

HOME = Path.home()
NAME = games.NAME
BASE = games.BASE
SCRIPTS = BASE / "scripts"
LOGS = BASE / "logs"
CONFIG = BASE / "config.json"
WINE_BIN = BASE / "wine" / "Wine Staging.app" / "Contents" / "Resources" / "wine" / "bin"
PREFIXES = {m: mods.prefix(m) for m, entry in games.MODS.items() if entry["runs"] == "wine"}
STEAM_MCC = Path("drive_c/Program Files (x86)/Steam/steamapps/common/Halo The Master Chief Collection")
APP = Path(os.environ.get("DROPPOD_APP_PATH")
           or Path(os.environ.get("DROPPOD_APPS_DIR", HOME / "Applications")) / f"{NAME}.app")
DESKTOP = Path(os.environ.get("DROPPOD_DESKTOP", HOME / "Desktop"))
# the games this install starts: each mod's process as the catalogue gives it (Project Reclaimer under its Wine,
# started from mods/reclaimer/; ChupathingyCE, a Mac app of its own). Only processes of this install count: the same
# programs started from another folder (a separate Project Reclaimer install) are left alone.
GAME_PROCESSES = [m["process"] for m in games.MODS.values()]
DAEMON = "local.droppod.loopback"
DAEMON_PLIST = f"/Library/LaunchDaemons/{DAEMON}.plist"
INSTALLED_PLIST = Path(os.environ.get("DROPPOD_TEST_DAEMON_PLIST", DAEMON_PLIST))  # (tests point it elsewhere)
DAEMON_DIR = "/Library/Application Support/droppod"
DRYRUN = os.environ.get("DROPPOD_DRY_RUN", "")  # "1": print admin commands; "cancel": as if cancelled
# native Halo CE: the catalogue's mod that plays from the player's Xbox disc (ChupathingyCE, GPL-3), and its own data
# folder on a Mac (maps, settings, saves): where its first start puts the maps
NATIVE_MOD = next(m for m, entry in games.MODS.items() if entry["setup"] == "xbox-disc")
NATIVE_DATA = Path(os.environ.get("DROPPOD_NATIVE_CE_DATA", HOME / "Library" / "Application Support" / "ChupathingyCE"))


def say(line):
    print(line, flush=True)


def as_admin(command, prompt):
    """Run a shell command as root behind macOS's own password dialog."""
    if DRYRUN:
        say("dry-run admin: " + command)
        return DRYRUN != "cancel"
    script = 'do shell script {} with prompt {} with administrator privileges'.format(
        json.dumps(command), json.dumps(prompt))
    return subprocess.run(["/usr/bin/osascript", "-e", script], capture_output=True).returncode == 0


def quoted(path):
    return "'" + str(path).replace("'", "'\\''") + "'"


def config():
    try:
        return json.loads(CONFIG.read_text())
    except (OSError, ValueError):
        return {}


def our_pids(pattern):
    """PIDs matching the pattern whose working folder (or program) is inside this install."""
    found = subprocess.run(["pgrep", "-a", "-f", pattern], capture_output=True, text=True).stdout.split()
    pids = []
    for pid in found:
        cwd = subprocess.run(["lsof", "-a", "-p", pid, "-d", "cwd", "-Fn"], capture_output=True, text=True).stdout
        cmd = subprocess.run(["ps", "-o", "command=", "-p", pid], capture_output=True, text=True).stdout
        if any(line == "n" + str(BASE) or line.startswith("n" + str(BASE) + "/") for line in cwd.splitlines()) \
                or str(BASE) in cmd:
            pids.append(pid)
    return pids


def running_games():
    return [p for p in GAME_PROCESSES if our_pids(p)]


# ---------------------------------------------------------------------------------------------------------------
def loopback_prefixes():
    """The address ranges the installed droppod-loopback.sh adds (e.g. 127.0.2), so a new range is checked too."""
    try:
        m = re.search(r"for p in ([0-9. ]+); do", (SCRIPTS / "droppod-loopback.sh").read_text())
        if m:
            return m.group(1).split()
    except OSError:
        pass
    return ["127.0.0", "127.0.1", "127.0.2", "127.0.3", "127.3.1", "127.3.2", "127.3.3"]


def net_check():
    have = set(re.findall(r"inet (127\.\d+\.\d+\.\d+)",
                          subprocess.run(["/sbin/ifconfig", "lo0"], capture_output=True, text=True).stdout))
    # one sample address per range (127.0.0.1 is always there, so 127.0.0.2 stands for its range)
    expected = [p + (".2" if p == "127.0.0" else ".1") for p in loopback_prefixes()]
    return "ok" if all(a in have for a in expected) else "missing"


def net_fix():
    """Install the startup task's files again and restart it, which re-adds every address (as install.sh does)."""
    script, plist = SCRIPTS / "droppod-loopback.sh", SCRIPTS / f"{DAEMON}.plist"
    if not script.exists() or not plist.exists():
        return "error: This install is missing its network files. Use Settings > Repair."
    command = (f"mkdir -p {quoted(DAEMON_DIR)} && "
               f"install -o root -g wheel -m 755 {quoted(script)} {quoted(DAEMON_DIR + '/droppod-loopback.sh')} && "
               f"install -o root -g wheel -m 644 {quoted(plist)} {DAEMON_PLIST} && "
               f"{{ launchctl bootout system/{DAEMON} 2>/dev/null; true; }} && "
               f"launchctl bootstrap system {DAEMON_PLIST}")
    if not as_admin(command, f"{NAME} is fixing the network settings Halo 3 needs."):
        return "error: The network setup wasn't fixed (the password box was cancelled)."
    if DRYRUN:
        return "ok"
    for _ in range(20):  # the task adds ~1,700 addresses; give it a moment
        if net_check() == "ok":
            return "ok"
        time.sleep(1)
    return f"error: The network setup is still incomplete. Restart your Mac, then open {NAME} again."


def gamecontrollerd_state():
    """The process state of macOS's game controller service (e.g. "Ss"), or None when it isn't running."""
    ps = subprocess.run(["/bin/ps", "-axo", "stat=,comm="], capture_output=True, text=True).stdout
    for line in ps.splitlines():
        state, _, command = line.strip().partition(" ")
        if command.strip() == "/usr/libexec/gamecontrollerd":
            return state
    return None


def controller_check():
    """macOS's gamecontrollerd can get stuck in the kernel (state U, for good). Every app that looks for controllers
    then hangs, Wine's included, so no game sees one although Bluetooth shows it connected. Only a restart
    clears it. A moment of waiting is normal, so it counts only if the service is stuck on every look."""
    for i in range(4):
        if i:
            time.sleep(0.5)
        if "U" not in (gamecontrollerd_state() or ""):
            return "ok"
    return "stuck"


CRASHES = [
    ("stopped responding, so the game had to close",
     "The graphics card stopped responding, so the game closed. Press Play to start again; if it keeps happening, "
     "lower the resolution in the game's settings."),
    ("device lost", "The graphics card stopped responding, so the game closed. Press Play to start again."),
    (" panicked at ", "Project Reclaimer hit an error and closed. Press Play to start again. If it keeps happening, "
                      "use Settings > Make a report."),
    ("wine: Unhandled page fault", "The game hit an error and closed. Press Play to start again. If it keeps "
                                   "happening, use Settings > Make a report."),
]


def crash_check(game):
    if not re.fullmatch(r"[a-z0-9-]+", game):
        return "error: unknown game"
    try:
        text = (LOGS / f"{game}.log").read_text(errors="replace")[-200000:]
    except OSError:
        return "ok"
    for marker, message in CRASHES:
        if marker in text:
            return "crash: " + message
    return "ok"


def force_quit():
    # the games first, by force: a frozen game often comes with a stuck wineserver, which must not stop this
    for p in GAME_PROCESSES:
        pids = our_pids(p)
        if pids:
            subprocess.run(["kill", "-9"] + pids, capture_output=True)
    for prefix in PREFIXES.values():
        if (WINE_BIN / "wineserver").exists() and prefix.exists():
            try:
                subprocess.run([str(WINE_BIN / "wineserver"), "-k"], env=dict(os.environ, WINEPREFIX=str(prefix)),
                               capture_output=True, timeout=10)
            except subprocess.TimeoutExpired:
                pass
    subprocess.run(["pkill", "-9", "-f", str(BASE / "wine") + "/"], capture_output=True)
    for _ in range(10):
        if not running_games():
            return "ok"
        time.sleep(0.5)
    if sys.platform == "darwin":
        return "error: The game didn't stop. Restart your Mac (Apple menu > Restart) to close it."
    return "error: The game didn't stop. Restart your computer to close it."


def report():
    account = ""
    try:
        account = (BASE / "steam-account").read_text().strip()
    except OSError:
        pass

    def scrub(text):
        return text.replace(account, "<steam account>") if account else text
    c = config()
    summary = [f"{NAME} report, {time.strftime('%Y-%m-%d %H:%M')}"]
    for label, cmd in [("macOS", ["sw_vers", "-productVersion"]), ("Mac", ["sysctl", "-n", "machdep.cpu.brand_string"]),
                       ("Memory", ["sysctl", "-n", "hw.memsize"])]:
        value = subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
        summary.append(f"{label}: " + (f"{int(value) / 2**30:.0f} GB" if label == "Memory" and value.isdigit() else value))
    try:
        summary.append(f"{NAME} version: " + (BASE / "version").read_text().strip())
    except OSError:
        summary.append(f"{NAME} version: unknown")
    summary.append("Installed games: " + ", ".join(c.get("games", [])))
    summary.append("Game files: " + ("an existing MCC folder" if c.get("source") == "existing" else "downloaded here"))
    for m, entry in games.MODS.items():
        program = mods.program(m)
        summary.append(f"{entry['name']}: " + (program.name if program else "not downloaded"))
    summary.append(f"Free disk space: {shutil.disk_usage(BASE).free / 1e9:.0f} GB")
    summary.append("Network addresses: " + net_check())
    DESKTOP.mkdir(parents=True, exist_ok=True)
    path = DESKTOP / f"{NAME} report {time.strftime('%Y-%m-%d %H.%M')}.zip"
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("summary.txt", scrub("\n".join(summary) + "\n"))
        for log in sorted(LOGS.glob("*.log")):
            z.writestr(f"logs/{log.name}", scrub(log.read_text(errors="replace")[-2_000_000:]))
    return f"ok {path}"


def reset_graphics():
    if running_games():  # (the game writes its settings when it quits)
        return "error: Quit the game first."
    for mod in (m for m, entry in games.MODS.items() if "presets" in entry.get("extras", [])):
        p = subprocess.run([sys.executable, "-I", str(SCRIPTS / "presets.py"), mod], capture_output=True, text=True)
        if p.returncode != 0:
            return "error: The graphics settings couldn't be changed."
    return "ok"


def forget_steam():
    (BASE / "steam-account").unlink(missing_ok=True)
    return "ok"


# A crashing engine writes crash_report\minidump.dmp (~3 GB) and crash_screenshot.bmp into the game files folder, as
# MCC itself does. No Steam depot ships that folder, so its dumps and screenshots can always go.
CRASH_FILES = ("*.dmp", "*.bmp")


def crash_files():
    folder = config().get("gameFiles")
    reports = Path(folder) / "crash_report" if folder else None
    if not reports or not reports.is_dir():
        return []
    return [f for pattern in CRASH_FILES for f in reports.glob(pattern) if f.is_file() and not f.is_symlink()]


def crash_reports():
    return str(sum(f.stat().st_size for f in crash_files()))


def clean_crash_reports():
    if running_games():  # (a game that is crashing right now may still be writing one)
        return "error: Quit the game first."
    freed = 0
    for f in crash_files():
        freed += f.stat().st_size
        f.unlink(missing_ok=True)
    return f"ok {freed}"


def repair():
    if running_games():
        return "error: Quit the game first."
    cleaned = clean_crash_reports()
    if cleaned != "ok 0":
        say(f"status: Deleted crash dumps ({int(cleaned.split()[1]) / 2**30:.1f} GB)")
    folder = config().get("gameFiles")
    if not folder or not Path(folder).is_dir():
        return "error: The game files folder isn't there. Is its drive connected?"
    chosen = set(games.installed(folder)) | set(config().get("wanted", []))  # (with the add-ons the player chose)
    depots = [d for d in games.needed(chosen) if d in set(games.present(folder))]
    if not depots:
        return "ok"  # (no game plays from these files yet: nothing to check)
    p = subprocess.run([sys.executable, "-I", str(SCRIPTS / "steam.py"), "download", folder] + [str(d) for d in depots])
    return "ok" if p.returncode == 0 else "error: Repair didn't finish. Press it again to pick up where it stopped."


def link_game_files(folder):
    """The Wine mods (Project Reclaimer) find MCC in their prefix's Steam folder, so that's a link to the game files."""
    for prefix in PREFIXES.values():
        link = prefix / STEAM_MCC
        if not prefix.exists():
            continue
        link.parent.mkdir(parents=True, exist_ok=True)
        if link.is_symlink() or not link.exists():
            link.unlink(missing_ok=True)
            link.symlink_to(folder)


def set_game_files(folder):
    folder = Path(folder).expanduser().resolve()
    if running_games():
        return "error: Quit the game first."
    if not folder.is_dir() or not games.present(folder):
        return "error: That folder doesn't have the Halo game files in it. Choose the folder with Data and the game folders (halo2, halo3) in it."
    c = config()
    games.write_config(CONFIG, folder, c.get("source", "existing"), set(c.get("wanted", c.get("games", []))))
    link_game_files(folder)
    return "ok " + " ".join(games.installed(folder))


def native_ce_status():
    import xiso
    if not mods.program(NATIVE_MOD):
        return "needs-port"
    return "ready" if xiso.verify(NATIVE_DATA) == "ok" else "needs-maps"


def native_ce_import(image):
    """The maps from the player's own disc image, into the folder ChupathingyCE reads them from."""
    import xiso
    if xiso.verify(NATIVE_DATA) == "ok":
        return "ok already"  # (imported before, by this app or by the port's own first start)
    try:
        xiso.check(image)
        say("status: Copying Halo's maps from your disc image…")
        xiso.extract(image, NATIVE_DATA)
    except xiso.ImageError as e:
        return f"error: {e}"
    except OSError as e:
        return f"error: Copying the maps failed ({e.strerror or e}). Check there's space, then try again."
    checked = xiso.verify(NATIVE_DATA)
    return "ok" if checked == "ok" else checked


def uninstall(delete_game_files=False):
    if not (BASE / "config.json").exists() and not (BASE / "wine").exists():
        return f"error: {BASE} doesn't look like a {NAME} install, so nothing was removed."
    stopped = force_quit()
    if stopped != "ok":
        return stopped  # (nothing deleted while a game still runs)
    c = config()
    folder = Path(c.get("gameFiles", "")) if c.get("gameFiles") else None
    clean_crash_reports()  # (the games' crash dumps go even when the game files stay)
    # the game files only when asked, and only a folder this app made for them (never a folder the player had)
    if delete_game_files and folder and c.get("ownsGameFiles") and folder.is_dir() and games.present(folder):
        shutil.rmtree(folder, ignore_errors=True)
    shutil.rmtree(APP, ignore_errors=True)
    shutil.rmtree(BASE, ignore_errors=True)
    if sys.platform == "linux":  # setup.py's app-menu entry
        share = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local" / "share")
        (share / "applications/droppod.desktop").unlink(missing_ok=True)
        (share / "icons/hicolor/256x256/apps/droppod.png").unlink(missing_ok=True)
    if not os.environ.get("DROPPOD_KEEP_NETWORK") and INSTALLED_PLIST.exists():
        removed = as_admin(f"launchctl bootout system/{DAEMON} 2>/dev/null; rm -f {DAEMON_PLIST}; "
                           f"rm -rf {quoted(DAEMON_DIR)}", f"{NAME} is removing its network setting.")
        if not removed:
            return "ok network-kept"  # (harmless: extra local addresses that nothing else uses)
    return "ok"


COMMANDS = {"net-check": net_check, "net-fix": net_fix, "controller-check": controller_check,
            "force-quit": force_quit, "report": report, "reset-graphics": reset_graphics,
            "forget-steam": forget_steam, "repair": repair,
            "crash-reports": crash_reports, "clean-crash-reports": clean_crash_reports,
            "native-ce-status": native_ce_status}


def main(argv):
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(1))
    if len(argv) == 3 and argv[1] == "crash-check":
        result = crash_check(argv[2])
    elif len(argv) == 4 and argv[1] == "play-with":
        try:
            games.play_with(CONFIG, argv[2], argv[3])
            result = "ok"
        except (OSError, ValueError) as e:
            result = f"error: {e}"
    elif len(argv) == 3 and argv[1] == "native-ce-import":
        result = native_ce_import(argv[2])
    elif len(argv) == 3 and argv[1] == "set-game-files":
        result = set_game_files(argv[2])
    elif len(argv) in (2, 3) and argv[1] == "uninstall":
        result = uninstall(delete_game_files=argv[2:] == ["delete-game-files"])
    elif len(argv) == 2 and argv[1] == "net-check":
        result = net_check()
        for _ in range(8):  # right after login the startup task may still be adding its addresses
            if result == "ok":
                break
            time.sleep(1)
            result = net_check()
    elif len(argv) == 2 and argv[1] in COMMANDS:
        result = COMMANDS[argv[1]]()
    else:
        print(__doc__)
        return 2
    say(result)
    return 1 if result.startswith("error: ") else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
