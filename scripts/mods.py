#!/usr/bin/env python3
"""Download and start the mods in the catalogue (mods.json; games.py reads it too).

  mods.py get <mod>             download the mod's latest release into mods/<mod>/ unless it's there, checked against
                                the release's checksum file (or, for a mod without one, the SHA-256 GitHub shows on
                                the release page); resumes a stopped download and checks the free space first. Progress lines, then "ok", "ok already" or "error: <plain reason>"
  mods.py program <mod>         the mod's installed program (mods/<mod>/<file>), or nothing when it isn't there
  mods.py which <game> [mod]    "<mod> <wine|mac>": what the game's Play button starts (the choice if the game has it,
                                else the game's first mod)
  mods.py launch <mod>          shell lines for run_wine.sh: WINEPREFIX, MOD_DIR (the folder in mods/<mod>/ it starts
                                from), EXE, ARGS, the mod's DLL overrides and environment
  mods.py prepare <mod>         restore chromehoist into the newest WebView2 runtime before a Mac launch

A mod that stops itself under Wine (notUnderWine in the catalogue) is neither downloaded nor started: get and launch
say why.

Downloaded once, from the mod's own GitHub releases; after that each mod updates itself when it starts (Project
Reclaimer in its own window, Project Gravemind from its menu, ChupathingyCE by pointing the player to its releases), so get leaves an installed mod
alone, except in a repair (DROPPOD_REPAIR=1). DROPPOD_DRY_RUN=1 says what get would download without connecting.
DROPPOD_RELEASES=<url> stands in for https://api.github.com/repos (tests).
"""
import hashlib
import http.client
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent))
import games  # noqa: E402  (the catalogue, the app's folder)

BASE = games.BASE
MODS = games.MODS
API = os.environ.get("DROPPOD_RELEASES", "https://api.github.com/repos")
DRY = os.environ.get("DROPPOD_DRY_RUN", "")
UA = {"User-Agent": "droppod"}


class Offline(Exception):
    pass


def say(line):
    print(line, flush=True)


def folder(mod):
    return BASE / "mods" / mod


def prefix(mod):
    return BASE / "prefixes" / mod


def version_key(name):
    return tuple(int(x) for x in re.findall(r"\d+", name))


def here(mod, key):
    """The catalogue's asset or program pattern for this system (its "windows"/"linux" entry, else the Mac's)."""
    m = MODS[mod]
    return m.get(games.PLATFORM, {}).get(key) or (m["source"]["asset"] if key == "asset" else m[key])


def program(mod):
    """The installed program: the newest-named file in mods/<mod>/ (or the catalogue's folder within it) that the
    catalogue's pattern matches."""
    pattern = re.compile(here(mod, "program"))
    try:
        found = [p for p in (folder(mod) / MODS[mod].get("folder", "")).iterdir() if pattern.match(p.name)]
    except OSError:
        return None
    return max(found, key=lambda p: version_key(p.name), default=None)


def fetch(url, timeout=20, headers=None):
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers={**UA, **(headers or {})}), timeout=timeout) as r:
            return r.read()
    except (urllib.error.URLError, OSError) as e:
        raise Offline(str(getattr(e, "reason", e))) from e


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def download(url, target, expected, what, size=0, room=1.0):
    """Download to target with progress lines, keeping it only if its SHA-256 matches. A stopped download resumes from
    its .part file. Before it starts, the space left must hold the rest `room` times (2: an archive and what it
    unpacks to)."""
    part = target.with_name(target.name + ".part")
    have = part.stat().st_size if part.exists() else 0
    if size and have >= size:  # (a finished .part that never got checked, or a different file of that name)
        part.unlink()
        have = 0
    free = shutil.disk_usage(target.parent).free
    if size and free < (size - have) * room + 100e6:
        raise RuntimeError(f"there isn't enough free space for {what}: it needs about "
                           f"{(size - have) * room / 1e6:.0f} MB, and {free / 1e6:.0f} MB are free")
    headers = dict(UA, Range=f"bytes={have}-") if have else UA
    try:
        r = urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=30)
    except urllib.error.HTTPError as e:
        if e.code != 416:  # (416: the range is past the end: start again)
            raise Offline(str(e)) from e
        part.unlink(missing_ok=True)
        return download(url, target, expected, what, size, room)
    except (urllib.error.URLError, OSError) as e:
        raise Offline(str(getattr(e, "reason", e))) from e
    resumed = have and r.status == 206
    with r, open(part, "ab" if resumed else "wb") as f:
        done = have if resumed else 0
        total = done + int(r.headers.get("Content-Length") or 0)
        shown = -1
        try:
            for block in iter(lambda: r.read(1 << 18), b""):
                f.write(block)
                done += len(block)
                pct = done * 100 // total if total else 0
                if pct != shown:
                    say(f"progress: {pct}")
                    shown = pct
        except (OSError, http.client.HTTPException) as e:  # (a dropped connection: the .part stays for the next try)
            raise Offline(str(e)) from e
        if done < total:
            raise Offline("the connection dropped")
    if sha256(part) != expected:
        part.unlink()
        raise RuntimeError(f"the {what} download didn't match its published checksum")
    part.replace(target)


def unpack(archive, dest):
    """Unpack a zip into dest. ditto keeps a Mac app's permissions, but takes Windows-style paths (with backslashes,
    as Reach Renaissance's zip has them) as file names; those are unpacked here, inside dest only."""
    with zipfile.ZipFile(archive) as z:
        windows = any("\\" in i.filename for i in z.infolist())
        if windows:
            dest.mkdir(parents=True)
            for i in z.infolist():
                path = PurePosixPath(i.filename.replace("\\", "/"))
                if path.is_absolute() or ".." in path.parts:
                    raise RuntimeError(f"the download has a file outside its folder ({i.filename})")
                if i.filename.endswith(("/", "\\")):
                    (dest / path).mkdir(parents=True, exist_ok=True)
                    continue
                (dest / path).parent.mkdir(parents=True, exist_ok=True)
                with z.open(i) as src, open(dest / path, "wb") as out:
                    shutil.copyfileobj(src, out)
    if not windows and games.PLATFORM != "mac":
        with zipfile.ZipFile(archive) as z:
            z.extractall(dest)  # (drops paths outside dest)
    elif not windows:
        subprocess.run(["/usr/bin/ditto", "-x", "-k", str(archive), str(dest)], check=True)


def latest(mod):
    """(asset name, its download URL, its size, its SHA-256, the release's tag) of the mod's latest release."""
    source = MODS[mod]["source"]
    release = json.loads(fetch(f"{API}/{source['github']}/releases/latest").decode())
    assets = {a["name"]: a for a in release.get("assets", [])}
    pattern = re.compile(here(mod, "asset"))
    names = [n for n in assets if pattern.match(n)]
    if not names or ("sums" in source and source["sums"] not in assets):
        raise RuntimeError(f"{MODS[mod]['name']}'s latest release has no download for this {'Mac' if games.PLATFORM == 'mac' else 'computer'}")
    name = names[0]
    sums = {}
    if "sums" in source:
        for line in fetch(assets[source["sums"]]["browser_download_url"]).decode().replace("\r", "").splitlines():
            parts = line.split()
            if len(parts) == 2:
                sums[parts[1].lstrip("*")] = parts[0].lower()
    elif (assets[name].get("digest") or "").startswith("sha256:"):  # (no checksum file: the one on its release page)
        sums[name] = assets[name]["digest"].split(":", 1)[1].lower()
    if name not in sums:
        raise RuntimeError(f"{MODS[mod]['name']}'s latest release doesn't list a checksum for {name}")
    a = assets[name]
    return name, a["browser_download_url"], int(a.get("size") or 0), sums[name], release.get("tag_name", "")


def get(mod):
    m = MODS[mod]
    if not games.runs_here(mod):
        return f"error: {m['notUnderWine']}"
    if program(mod) and not os.environ.get("DROPPOD_REPAIR"):
        return "ok already"  # (it updates itself from here on)
    if DRY:
        say(f"dry-run: download the latest {m['name']} release from github.com/{m['source']['github']} "
            f"(checked against its {m['source'].get('sums', 'SHA-256 on GitHub')}) into {folder(mod)}")
        return "ok"
    name, url, size, expected, tag = latest(mod)
    downloads = BASE / "downloads"
    downloads.mkdir(parents=True, exist_ok=True)
    folder(mod).mkdir(parents=True, exist_ok=True)
    say(f"status: Downloading {m['name']} {tag.lstrip('v')} ({size / 1e6:.0f} MB)…")
    archive = name.endswith(".zip")
    download(url, downloads / name, expected, m["name"], size, room=2 if archive else 1)
    if archive:  # a Mac app, or a mod's folder (Project Gravemind): unpacked beside the old one, then swapped in
        new = folder(mod).with_name(mod + ".new")
        shutil.rmtree(new, ignore_errors=True)
        unpack(downloads / name, new)
        (downloads / name).unlink()
        shutil.rmtree(folder(mod), ignore_errors=True)
        new.replace(folder(mod))
    else:  # one program among the mod's other files (Mesa's DLLs for Project Reclaimer): older ones go
        (downloads / name).replace(folder(mod) / name)
        pattern = re.compile(here(mod, "program"))
        for old in folder(mod).iterdir():
            if old.name != name and pattern.match(old.name):
                old.unlink()
    if games.PLATFORM == "linux" and program(mod):
        program(mod).chmod(0o755)  # (zips don't keep it executable)
    return "ok" if program(mod) else f"error: {m['name']}'s download didn't have its program in it"


def which(game, choice=""):
    options = games.PLAY_WITH.get(game)
    if not options:
        raise ValueError(f"unknown game {game}")
    mod = choice if choice in options else options[0]
    return f"{mod} {MODS[mod]['runs']}"


def prepare(mod):
    """Evergreen WebView2 updates create a new version folder without our menu fix. Keep its original DLL as the
    forwarding target; replace only the proxy, atomically, so a failed copy can be retried."""
    if "chromehoist" not in MODS[mod].get("extras", []):
        return
    edge = prefix(mod) / "drive_c/Program Files (x86)/Microsoft/EdgeWebView/Application"
    versions = [d for d in edge.iterdir() if d.is_dir() and re.fullmatch(r"\d+(?:\.\d+)+", d.name)] if edge.is_dir() else []
    if not versions:
        raise ValueError("No WebView2 runtime in the prefix. Open Drop Pod and use Settings > Repair")
    target = max(versions, key=lambda d: version_key(d.name))
    ours = BASE / "chromehoist/msedge_elf.dll"
    if not ours.is_file():
        raise ValueError("The WebView2 menu fix is missing. Open Drop Pod and use Settings > Repair")
    proxy, original = target / "msedge_elf.dll", target / "msedge_elf_orig.dll"
    if proxy.is_file() and proxy.read_bytes() == ours.read_bytes() and original.is_file():
        return
    if proxy.is_file() and proxy.stat().st_size > 1_000_000:
        proxy.replace(original)
    if not original.is_file():
        raise ValueError(f"WebView2 {target.name} has no original msedge_elf.dll to forward to. Reinstall its runtime")
    new = proxy.with_suffix(".dll.new")
    shutil.copyfile(ours, new)
    new.replace(proxy)
    say(f"chromehoist installed into WebView2 {target.name}")


def launch(mod):
    """What run_wine.sh needs, as shell lines it evaluates. Environment values the player already set win."""
    m = MODS[mod]
    if m["runs"] != "wine":
        raise ValueError(f"{mod} doesn't run under Wine")
    if not games.runs_here(mod):
        raise ValueError(m["notUnderWine"])
    exe = program(mod)
    lines = [f"export WINEPREFIX={shlex.quote(str(prefix(mod)))}",
             f"MOD_DIR={shlex.quote(m.get('folder', ''))}",
             f"EXE={shlex.quote(exe.name if exe else '')}",
             "ARGS=(" + " ".join(shlex.quote(a) for a in m.get("args", [])) + ")",
             f"MOD_OVERRIDES={shlex.quote(m.get('dllOverrides', ''))}"]
    lines += [f"export {k}=${{{k}:-{shlex.quote(v)}}}" for k, v in m.get("env", {}).items()]
    return "\n".join(lines)


def main(argv):
    try:
        if len(argv) == 3 and argv[1] == "get" and argv[2] in MODS:
            try:
                result = get(argv[2])
            except Offline as e:
                result = f"error: Couldn't download {MODS[argv[2]]['name']} ({e}). Check your internet connection " \
                         "and try again: it picks up where it stopped."
            except (RuntimeError, OSError, subprocess.CalledProcessError) as e:
                result = f"error: {str(e)[:1].upper() + str(e)[1:]}."
            say(result)
            return 1 if result.startswith("error: ") else 0
        if len(argv) == 3 and argv[1] == "program" and argv[2] in MODS:
            p = program(argv[2])
            if p:
                print(p)
        elif len(argv) in (3, 4) and argv[1] == "which":
            print(which(argv[2], argv[3] if len(argv) == 4 else ""))
        elif len(argv) == 3 and argv[1] == "launch" and argv[2] in MODS:
            print(launch(argv[2]))
        elif len(argv) == 3 and argv[1] == "prepare" and argv[2] in MODS:
            prepare(argv[2])
        else:
            print(__doc__)
            return 2
    except (ValueError, OSError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
