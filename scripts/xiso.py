#!/usr/bin/env python3
"""Halo's maps folder out of the player's own Xbox disc image, for the native Halo CE port (ChupathingyCE).

  xiso.py check <image>          "ok <maps folder> <files> <bytes>": an Xbox disc image with a Halo maps folder, or
                                 "error: <plain reason>"
  xiso.py extract <image> <dir>  copy the image's maps folder to <dir>/maps, through <dir>/maps.partial so a stopped
                                 copy never looks complete: "progress: <0-100>" lines, then "ok" or "error: ..."
  xiso.py verify <dir>           "ok" when <dir>/maps holds the Xbox game's maps (its ui.map), else "error: ..."

ChupathingyCE's own first start does the same when it has no maps: it asks for the image and copies its maps folder
to ~/Library/Application Support/ChupathingyCE/maps. The app does it beforehand, with its own progress bar and
checks, so the port starts straight into the game.

The image's file system is XDVDFS (as documented by the Xbox homebrew community, e.g. extract-xiso and xboxdevwiki):
2048-byte sectors; a volume descriptor 0x10000 bytes into the game partition, with "MICROSOFT*XBOX*MEDIA" at its
start and at 0x7EC, and the root directory's sector and size after the first; directories stored as binary trees of
entries (left and right subtree offsets in 4-byte units, start sector, size, attributes, name length, name). Images of
a whole disc put the game partition further in. Written for Drop Pod (MIT); it reads only the maps folder.
"""
import os
import shutil
import struct
import sys
from pathlib import Path

SECTOR = 2048
MAGIC = b"MICROSOFT*XBOX*MEDIA"
DESCRIPTOR = 0x10000
PARTITIONS = (0, 0x0FD90000, 0x02080000, 0x18300000)  # a game-partition image, then whole-disc layouts
DIRECTORY = 0x10
MAX_TABLE = 4 << 20  # directory tables are a few sectors; anything bigger isn't one
MAX_ENTRIES = 4096
XBOX_UI_VERSION = 5  # ui.map's header version on the Xbox game; Halo PC's is 7, Custom Edition's 609


class ImageError(Exception):
    pass


class Image:
    def __init__(self, path):
        try:
            self.file = open(path, "rb")
        except OSError as e:
            raise ImageError(f"The disc image couldn't be opened ({e.strerror}).") from e
        for partition in PARTITIONS:
            self.file.seek(partition + DESCRIPTOR)
            d = self.file.read(SECTOR)
            if len(d) == SECTOR and d[:20] == MAGIC and d[0x7EC:0x7EC + 20] == MAGIC:
                self.partition = partition
                self.root = struct.unpack_from("<II", d, 20)
                return
        self.file.close()
        raise ImageError("This isn't an Xbox disc image. Choose the .iso (or .xiso) made from your Halo disc.")

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.file.close()

    def entries(self, sector, size):
        """(name, sector, size, is_directory) of a directory's entries."""
        if not 0 < size <= MAX_TABLE:
            raise ImageError("The disc image is damaged (a folder can't be read).")
        self.file.seek(self.partition + sector * SECTOR)
        table = self.file.read(size)
        found, seen, todo = [], set(), [0]
        while todo and len(seen) < MAX_ENTRIES:
            offset = todo.pop() * 4
            if offset in seen or offset + 14 > len(table):
                continue
            seen.add(offset)
            left, right, start, length, attributes, name_length = struct.unpack_from("<HHIIBB", table, offset)
            if left == 0xFFFF:  # padding: an empty directory
                continue
            name = table[offset + 14:offset + 14 + name_length].decode("latin-1")
            if name:
                found.append((name, start, length, bool(attributes & DIRECTORY)))
            todo += [o for o in (left, right) if o]
        return found

    def maps_folder(self):
        """The root's maps folder: "maps", or on a multi-language disc the English one (else the first)."""
        folders = [e for e in self.entries(*self.root) if e[3]]
        by_name = {e[0].lower(): e for e in folders}
        choice = by_name.get("maps") or by_name.get("maps_en") or next(
            (e for e in sorted(folders) if e[0].lower().startswith("maps_")), None)
        if not choice:
            raise ImageError("This disc image has no maps folder, so it isn't Halo: Combat Evolved.")
        files = [e for e in self.entries(choice[1], choice[2]) if not e[3] and plain(e[0])]
        if not any(f[0].lower() == "ui.map" for f in files):
            raise ImageError("This disc image's maps folder has no ui.map, so it isn't Halo: Combat Evolved.")
        return choice[0], files

    def copy(self, sector, size, target, progress):
        self.file.seek(self.partition + sector * SECTOR)
        with open(target, "wb") as out:
            left = size
            while left:
                block = self.file.read(min(left, 1 << 20))
                if not block:
                    raise ImageError("The disc image ends early: it's incomplete or damaged.")
                out.write(block)
                left -= len(block)
                progress(len(block))


def plain(name):
    """A name safe to write in the maps folder (a disc's maps folder holds .map files and loading.tga)."""
    return name not in (".", "..") and all(0x20 <= ord(c) <= 0x7E and c not in '/\\:*?"<>|' for c in name)


def check(image):
    with Image(image) as img:
        name, files = img.maps_folder()
    return f"ok {name} {len(files)} {sum(f[2] for f in files)}"


def extract(image, destination):
    with Image(image) as img:
        return _extract(img, destination)


def _extract(img, destination):
    _, files = img.maps_folder()
    destination = Path(destination)
    total = sum(f[2] for f in files) or 1
    destination.mkdir(parents=True, exist_ok=True)
    free = shutil.disk_usage(destination).free
    if free < total + (256 << 20):
        raise ImageError(f"Not enough space: Halo's maps need {total / 2**30:.1f} GB and there's "
                         f"{free / 2**30:.1f} GB free. Free up some space, then try again.")
    partial, final = destination / "maps.partial", destination / "maps"
    shutil.rmtree(partial, ignore_errors=True)
    partial.mkdir()
    done, shown = [0], [-1]

    def progress(n):
        done[0] += n
        pct = done[0] * 100 // total
        if pct != shown[0]:
            print(f"progress: {pct}", flush=True)
            shown[0] = pct
    for name, sector, size, _ in files:
        img.copy(sector, size, partial / name, progress)
    if final.exists():  # (a maps folder that didn't check out: kept aside, never deleted)
        final.rename(destination / f"maps.before-{os.getpid()}")
    partial.rename(final)
    return "ok"


def verify(destination):
    ui = Path(destination) / "maps" / "ui.map"
    try:
        with open(ui, "rb") as f:
            header = f.read(8)
    except OSError:
        return "error: Halo's maps aren't there yet."
    if len(header) < 8 or header[:4] != b"daeh":
        return "error: The maps folder's ui.map isn't a Halo map."
    version = struct.unpack_from("<i", header, 4)[0]
    if version == XBOX_UI_VERSION:
        return "ok"
    if version in (7, 609):
        return "error: These are Halo PC maps. The native port needs the maps from the Xbox disc."
    return f"error: This ui.map (version {version}) isn't from the Xbox game."


def main(argv):
    try:
        if len(argv) == 3 and argv[1] == "check":
            result = check(argv[2])
        elif len(argv) == 4 and argv[1] == "extract":
            result = extract(argv[2], argv[3])
        elif len(argv) == 3 and argv[1] == "verify":
            result = verify(argv[2])
        else:
            print(__doc__)
            return 2
    except ImageError as e:
        result = f"error: {e}"
    except OSError as e:
        result = f"error: Copying the maps failed ({e.strerror or e}). Check there's space, then try again."
    print(result, flush=True)
    return 0 if result.startswith("ok") else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
