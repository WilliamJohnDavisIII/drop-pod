#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""make_notices.py: write THIRD_PARTY_NOTICES, what's built into the window (launcher/): every Rust crate it's
compiled from, with its licence, then each distinct licence text those crates ship, then the two fonts.
Run after changing launcher/Cargo.toml or Cargo.lock: python3 -I scripts/make_notices.py (needs cargo)."""
import hashlib
import json
import os
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CARGO = os.environ.get("CARGO") or str(Path.home() / ".cargo/bin/cargo")
LICENCE_FILES = ("license", "licence", "copying", "notice", "unlicense")


def crates():
    out = subprocess.run([CARGO, "metadata", "--format-version", "1", "--locked", "--offline",
                          "--manifest-path", str(REPO / "launcher/Cargo.toml")],
                         check=True, capture_output=True, text=True).stdout
    meta = json.loads(out)
    root = meta["resolve"]["root"]
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    # what the window is built from: normal dependencies, on any platform (Windows and Linux come next)
    seen, todo = set(), [root]
    while todo:
        n = nodes[todo.pop()]
        for d in n["deps"]:
            if any(k["kind"] in (None, "normal") for k in d["dep_kinds"]) and d["pkg"] not in seen:
                seen.add(d["pkg"])
                todo.append(d["pkg"])
    return sorted((p for p in meta["packages"] if p["id"] in seen), key=lambda p: (p["name"], p["version"]))


def texts(pkg):
    folder = Path(pkg["manifest_path"]).parent
    found = []
    for f in sorted(folder.iterdir()):
        if f.is_file() and f.name.lower().startswith(LICENCE_FILES):
            found.append(f.read_text(errors="replace").strip())
    if pkg.get("license_file"):
        f = folder / pkg["license_file"]
        if f.is_file():
            found.append(f.read_text(errors="replace").strip())
    return list(dict.fromkeys(found))


def main():
    pkgs = crates()
    out = ["THIRD-PARTY NOTICES: Drop Pod's window (launcher/)",
           "",
           "The window is written in Rust and is free software under the GNU General Public License, version 3 or",
           "later (LICENSE); parts ported from the Project Reclaimer Mac port stay MIT",
           "(LICENSE.reclaimer-mac-port). It is compiled from the crates below, each under its own licence, and",
           "contains the two fonts after them. Nothing of Halo's, MCC's or Bungie's is in it: the pictures behind",
           "the menus are read at run time from the player's own game files, and the menu sounds are made in code.",
           "What the installer downloads is listed in CREDITS.md.",
           "",
           f"CRATES ({len(pkgs)}, for every platform the window builds for)",
           ""]
    width = max(len(f"{p['name']} {p['version']}") for p in pkgs)
    for p in pkgs:
        out.append(f"  {p['name'] + ' ' + p['version']:<{width}}  {p.get('license') or 'see its licence file'}")
    out += ["", "FONTS", "",
            "  Michroma (launcher/assets/fonts/Michroma-Regular.ttf): SIL Open Font License 1.1.",
            "    Copyright 2011 The Michroma Project Authors (https://github.com/googlefonts/Michroma-font)",
            "  Titillium Web (launcher/assets/fonts/TitilliumWeb-Regular.ttf, -SemiBold.ttf): SIL Open Font License 1.1.",
            "    Copyright (c) 2009-2011 by Accademia di Belle Arti di Urbino and students of MA course of Visual design.",
            "", "", "LICENCE TEXTS", "",
            "Each text below once, with the crates that ship it.", ""]
    by_text = {}
    for p in pkgs:
        for t in texts(p):
            by_text.setdefault(hashlib.sha256(t.encode()).hexdigest(), (t, []))[1].append(f"{p['name']} {p['version']}")
    for t, users in sorted(by_text.values(), key=lambda v: v[1][0]):
        out += ["=" * 100, "Used by: " + ", ".join(users), "-" * 100, t, ""]
    fonts = REPO / "launcher/assets/fonts"
    for name, f in [("Michroma", "OFL-Michroma.txt"), ("Titillium Web", "OFL-TitilliumWeb.txt")]:
        out += ["=" * 100, f"Font: {name}", "-" * 100, (fonts / f).read_text().strip(), ""]
    (REPO / "THIRD_PARTY_NOTICES").write_text("\n".join(out) + "\n")
    print(f"THIRD_PARTY_NOTICES: {len(pkgs)} crates, {len(by_text)} licence texts")


if __name__ == "__main__":
    main()
