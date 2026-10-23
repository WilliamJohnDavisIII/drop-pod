#!/usr/bin/env python3
"""play.py <game> [mod]: what a library tile's Play button runs on Windows and Linux (play.sh on the Mac).
Experimental and untested. Output goes to logs/<game>.log (emptied first); it returns when the game has quit.

The mod is the tile's choice (config.json playWith.<game>), else the game's first in the catalogue. A Windows mod
starts as it is on Windows; on Linux it runs under the system's Wine, in its own prefix (prefixes/<mod>), with the
game files linked where it looks for them, as on the Mac. A native mod (ChupathingyCE) starts as it is.
"""
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import games  # noqa: E402
import mods  # noqa: E402

MCC_REL = Path("drive_c/Program Files (x86)/Steam/steamapps/common/Halo The Master Chief Collection")


def main(argv):
    if len(argv) not in (2, 3):
        print(__doc__)
        return 2
    game = argv[1]
    try:
        config = json.loads((games.BASE / "config.json").read_text())
    except (OSError, ValueError):
        config = {}
    choice = argv[2] if len(argv) == 3 else config.get("playWith", {}).get(game, "")
    log_path = games.BASE / "logs" / f"{game}.log"
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with open(log_path, "w", encoding="utf-8") as log:
        def fail(text):
            print(text, file=log)
            print(text, file=sys.stderr)
            return 1
        try:
            mod, runs = mods.which(game, choice).split()
        except ValueError as e:
            return fail(f"play.py: {e}")
        m = games.MODS[mod]
        exe = mods.program(mod)
        if not exe:
            return fail(f"{m['name']} isn't set up yet: open Settings > Repair")
        cmd, env = [str(exe)], dict(os.environ)
        if runs == "wine":
            cmd += m.get("args", [])
            env.update({k: env.get(k) or v for k, v in m.get("env", {}).items()})
            if games.PLATFORM == "linux":
                wine = shutil.which("wine")
                if not wine:
                    return fail(f"{m['name']} is a Windows program: install Wine (your distribution's wine package), "
                                "then press Play again")
                prefix = mods.prefix(mod)
                link = prefix / MCC_REL
                link.parent.mkdir(parents=True, exist_ok=True)
                if config.get("gameFiles") and (link.is_symlink() or not link.exists()):
                    link.unlink(missing_ok=True)
                    link.symlink_to(config["gameFiles"])
                env.update(WINEPREFIX=str(prefix), WINEDEBUG=env.get("WINEDEBUG", "-all"))
                if m.get("dllOverrides"):
                    env["WINEDLLOVERRIDES"] = m["dllOverrides"]
                cmd = [wine] + cmd
        try:
            rc = subprocess.run(cmd, cwd=exe.parent, env=env, stdin=subprocess.DEVNULL, stdout=log,
                                stderr=subprocess.STDOUT).returncode
        except OSError as e:
            return fail(f"play.py: couldn't start {exe.name} ({e})")
        if runs == "wine" and games.PLATFORM == "linux":
            # the mods restart themselves to update, under the same wineserver: wait for all of it
            subprocess.run(["wineserver", "-w"], env=env, stdout=log, stderr=subprocess.STDOUT)
        return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv))
