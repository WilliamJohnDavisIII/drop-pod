#!/usr/bin/env python3
"""The games, their mods and Steam depots (all from the mod catalogue, mods.json), which depots a game files folder
already has, and config.json.

  games.py present <dir>               depot IDs whose marker files are all in <dir>, one per line
  games.py missing <dir> <game>...     depot IDs the chosen games need that <dir> doesn't have, shared files first
  games.py installed <dir>             game IDs that play from MCC's files and whose depots are all in <dir>
  games.py describe <depot>...         "<depot> <GiB> <name>" per depot (for progress messages)
  games.py mods <game>...              "<mod> <wine|mac> <extras,...|-> <name>" per mod the chosen games play with
  (<game>... may include a game's optional add-ons, such as halo2-mp: chosen with their game, never alone)
  games.py write-config <config.json> <game files dir> <source> <wanted games...>
                                       write config.json; keeps its playWith entries and records which games are there
  games.py play-with <config.json> <game> <mod>   what that game's Play button runs (one of the game's mods)

mods.json, beside this file, is the one place the games and mods are listed; the window reads the same file. Per
game: its mods, the default first. Per mod: its games' depots, where it's downloaded from (a GitHub repo's latest
release, checked against the release's checksum file, or GitHub's own SHA-256 when it has none), its size, how it
runs (under Wine in its own prefix, or as a Mac app) and how it starts (program, arguments, DLL overrides,
environment), what install.sh adds for it (extras: mesa, spinfix, loopback, presets), and how it's set up (with its
game, or from the player's Xbox disc). A Windows mod with notUnderWine stops itself under Wine, so it can't run here:
a game with no other mod is left out of everything below (the window shows it with that message). mods.py downloads
and starts mods. Depots are English only.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

try:  # the app's name (APP_NAME, copied next to the scripts; the one place it's set)
    NAME = (Path(__file__).resolve().parent / "APP_NAME").read_text().strip() or "Drop Pod"
except OSError:
    NAME = "Drop Pod"
BASE = Path(os.environ.get("DROPPOD_HOME", Path.home() / "Library" / "Application Support" / NAME))
CATALOGUE = json.loads((Path(__file__).resolve().parent / "mods.json").read_text())
APP_ID = CATALOGUE["steamApp"]
# depot: (name, GiB as Steam shows it, marker files)
DEPOTS = {d["id"]: (d["name"], d["gib"], d["markers"]) for d in CATALOGUE["depots"]}
BASE_DEPOT = CATALOGUE["baseDepot"]
MODS = {m["id"]: m for m in CATALOGUE["mods"]}
# windows, linux or mac (DROPPOD_PLATFORM stands in, for dry runs): a mod's "windows"/"linux" entry, when it has one,
# gives that system's asset and program
PLATFORM = os.environ.get("DROPPOD_PLATFORM") or {"win32": "windows", "linux": "linux"}.get(sys.platform, "mac")


def runs_here(mod):
    """False for a Windows mod that stops itself under Wine (notUnderWine), except on Windows, where it runs natively."""
    return PLATFORM == "windows" or not (MODS[mod]["runs"] == "wine" and MODS[mod].get("notUnderWine"))


# What a game's Play button can run, the default first; a game with nothing that runs here isn't listed. The choice
# is config.json's playWith.<game>.
PLAY_WITH = {g["id"]: ms for g in CATALOGUE["games"] if (ms := [m for m in g["mods"] if runs_here(m)])}
# game: the depots its mods need (none: it doesn't play from MCC's files), in the catalogue's depot order
GAMES = {g: [d for d in DEPOTS if any(d in MODS[m]["depots"] for m in mods)] for g, mods in PLAY_WITH.items()}
# add-on: (its game, its depots): optional extra game files the player ticks with their game (Halo 2's multiplayer maps)
ADDONS = {a["id"]: (g["id"], a["depots"]) for g in CATALOGUE["games"] if g["id"] in PLAY_WITH
          for a in g.get("addons", [])}
# what a player can choose, in order: each game, then its add-ons
CHOICES = [c for g in GAMES for c in [g] + [a for a, (game, _) in ADDONS.items() if game == g]]

def present(folder):
    folder = Path(folder)
    return [d for d, (_, _, markers) in DEPOTS.items() if all((folder / m).is_file() for m in markers)]


def needed(chosen):
    """Every depot the chosen games (and their chosen add-ons) need, the shared one first (when any is needed), in a
    fixed order."""
    out = [d for c in CHOICES if c in chosen and (c in GAMES or ADDONS[c][0] in chosen)
           for d in (GAMES[c] if c in GAMES else ADDONS[c][1])]
    return [BASE_DEPOT] + out if out else []


def missing(folder, chosen):
    have = set(present(folder))
    return [d for d in needed(chosen) if d not in have]


def installed(folder):
    have = set(present(folder))
    if BASE_DEPOT not in have:
        return []
    return [g for g, depots in GAMES.items() if depots and all(d in have for d in depots)]


def windows_path(path):
    return "Z:" + str(path).replace("/", "\\")


def write_config(path, game_files, source, wanted):
    path = Path(path)
    try:
        old = json.loads(path.read_text())
    except (OSError, ValueError):
        old = {}
    owner = BASE / ".installed" / "game-files-owner"
    owns = owner.is_file() and owner.read_text().strip() == str(game_files)
    config = {
        "version": 1,
        "gameFiles": str(game_files),
        "gameFilesWindows": windows_path(game_files),
        "source": source,
        "ownsGameFiles": owns,
        "games": installed(game_files),
        "wanted": [c for c in CHOICES if c in wanted],
        "wine": str(BASE / "wine" / "Wine Staging.app" / "Contents" / "Resources" / "wine" / "bin" / "wine"),
    }
    if old.get("playWith"):
        config["playWith"] = old["playWith"]
    save(path, config)
    return config


def save(path, config):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    # a temp file renamed over: the window never reads a half-written file
    with tempfile.NamedTemporaryFile("w", dir=path.parent, prefix=".config.", suffix=".tmp", delete=False) as f:
        f.write(json.dumps(config, indent=2) + "\n")
    os.chmod(f.name, 0o644)
    os.replace(f.name, path)


def play_with(path, game, mod):
    if mod not in PLAY_WITH.get(game, []):
        raise ValueError(f"{game} can't play with {mod}")
    config = json.loads(Path(path).read_text())
    config.setdefault("playWith", {})[game] = mod
    save(path, config)


def mods_for(chosen):
    """The default mod of each chosen game, in the catalogue's order."""
    return [m for m in MODS if any(PLAY_WITH[g][0] == m for g in chosen if g in PLAY_WITH)]


def main(argv):
    cmd = argv[1] if len(argv) > 1 else ""
    if cmd == "present" and len(argv) == 3:
        for d in present(argv[2]):
            print(d)
    elif cmd == "missing" and len(argv) >= 3:
        for d in missing(argv[2], set(argv[3:])):  # (nothing at all when nothing is missing)
            print(d)
    elif cmd == "installed" and len(argv) == 3:
        for g in installed(argv[2]):
            print(g)
    elif cmd == "describe":
        for d in argv[2:]:
            name, gib, _ = DEPOTS[int(d)]
            print(f"{d} {gib} {name}")
    elif cmd == "mods":
        for m in mods_for(set(argv[2:])):
            print(m, MODS[m]["runs"], ",".join(MODS[m].get("extras", [])) or "-", MODS[m]["name"])
    elif cmd == "write-config" and len(argv) >= 5:
        c = write_config(argv[2], argv[3], argv[4], set(argv[5:]))
        print("games: " + (" ".join(c["games"]) or "none yet"))
    elif cmd == "play-with" and len(argv) == 5:
        play_with(argv[2], argv[3], argv[4])
        print("ok")
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
