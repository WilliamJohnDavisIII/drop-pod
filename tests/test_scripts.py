#!/usr/bin/env python3
"""Tests for scripts/xiso.py, scripts/games.py and mods.py (the catalogue), and tools.py's crash-report cleanup and repair.
Run: python3 -I -B tests/test_scripts.py (scripts/make_app.sh test runs it too)."""
import hashlib
import http.server
import io
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import unittest
import zipfile
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent / "scripts"
sys.path.insert(0, str(SCRIPTS))
import games  # noqa: E402
import mods  # noqa: E402
import xiso  # noqa: E402

SECTOR = 2048


def map_header(version):
    return b"daeh" + struct.pack("<i", version) + b"\0" * 2040


def entry(name, sector, size, directory=False, left=0, right=0):
    raw = name.encode()
    e = struct.pack("<HHIIBB", left, right, sector, size, 0x10 if directory else 0x20, len(raw)) + raw
    return e + b"\xff" * (-len(e) % 4)


def table(entries):
    """A directory table: entries chained through their right pointers (a degenerate but valid binary tree)."""
    out, offsets = b"", []
    for e in entries:
        offsets.append(len(out) // 4)
        out += e
    # fix up the right pointers now that offsets are known
    chunks = []
    for i, e in enumerate(entries):
        right = offsets[i + 1] if i + 1 < len(entries) else 0
        chunks.append(e[:2] + struct.pack("<H", right) + e[4:])
    return b"".join(chunks)


def build_image(path, partition=0, maps_name="maps", ui_version=5, files=None):
    """An XDVDFS image: root = default.xbe + <maps_name>/, which holds ui.map and a10.map."""
    files = files or {"ui.map": map_header(ui_version) + b"U" * 3000, "a10.map": b"A" * 5000}
    data_sector = 64
    maps_entries, blobs = [], []
    for name, blob in files.items():
        maps_entries.append(entry(name, data_sector, len(blob)))
        blobs.append((data_sector, blob))
        data_sector += (len(blob) + SECTOR - 1) // SECTOR + 1
    maps_table = table(maps_entries)
    root_table = table([entry("default.xbe", data_sector, 4), entry(maps_name, 50, len(maps_table), directory=True)])
    with open(path, "wb") as f:
        def put(sector, blob):
            f.seek(partition + sector * SECTOR)
            f.write(blob)
        descriptor = bytearray(SECTOR)
        descriptor[:20] = xiso.MAGIC
        struct.pack_into("<II", descriptor, 20, 40, len(root_table))
        descriptor[0x7EC:0x7EC + 20] = xiso.MAGIC
        f.seek(partition + xiso.DESCRIPTOR)
        f.write(descriptor)
        put(40, root_table)
        put(50, maps_table)
        for sector, blob in blobs:
            put(sector, blob)
        put(data_sector, b"XBEH")
    return files


class Xiso(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="droppod-xiso-"))

    def tearDown(self):
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_check_extract_verify(self):
        image = self.tmp / "halo.iso"
        files = build_image(image)
        self.assertEqual(xiso.check(image), f"ok maps 2 {sum(len(b) for b in files.values())}")
        dest = self.tmp / "ChupathingyCE"
        self.assertEqual(xiso.extract(image, dest), "ok")
        for name, blob in files.items():
            self.assertEqual((dest / "maps" / name).read_bytes(), blob)
        self.assertFalse((dest / "maps.partial").exists())
        self.assertEqual(xiso.verify(dest), "ok")

    def test_whole_disc_image(self):
        image = self.tmp / "halo-redump.iso"
        build_image(image, partition=0x02080000)  # (sparse: only the written parts take space)
        self.assertTrue(xiso.check(image).startswith("ok maps 2 "))

    def test_multi_language_disc(self):
        image = self.tmp / "halo-eu.iso"
        build_image(image, maps_name="maps_en")
        self.assertTrue(xiso.check(image).startswith("ok maps_en "))

    def test_not_an_image(self):
        bad = self.tmp / "notes.iso"
        bad.write_bytes(b"\0" * 200000)
        with self.assertRaises(xiso.ImageError) as e:
            xiso.check(bad)
        self.assertIn("isn't an Xbox disc image", str(e.exception))

    def test_not_halo(self):
        image = self.tmp / "other.iso"
        build_image(image, maps_name="media")
        with self.assertRaises(xiso.ImageError) as e:
            xiso.check(image)
        self.assertIn("no maps folder", str(e.exception))

    def test_pc_maps_refused(self):
        image = self.tmp / "pc.iso"
        build_image(image, ui_version=7)
        dest = self.tmp / "data"
        xiso.extract(image, dest)
        self.assertIn("Halo PC maps", xiso.verify(dest))

    def test_old_maps_kept_aside(self):
        image = self.tmp / "halo.iso"
        build_image(image)
        dest = self.tmp / "data"
        (dest / "maps").mkdir(parents=True)
        (dest / "maps" / "old.txt").write_text("keep me")
        xiso.extract(image, dest)
        aside = [p for p in dest.iterdir() if p.name.startswith("maps.before-")]
        self.assertEqual(len(aside), 1)
        self.assertEqual((aside[0] / "old.txt").read_text(), "keep me")

    def test_command_line(self):
        image = self.tmp / "halo.iso"
        build_image(image)
        out = subprocess.run([sys.executable, "-I", "-B", str(SCRIPTS / "xiso.py"), "extract", str(image),
                              str(self.tmp / "d")], capture_output=True, text=True).stdout.splitlines()
        self.assertEqual(out[-1], "ok")
        self.assertIn("progress: 100", out)


class Games(unittest.TestCase):
    def run_games(self, *args):
        return subprocess.run([sys.executable, "-I", "-B", str(SCRIPTS / "games.py"), *args],
                              capture_output=True, text=True).stdout

    def test_nothing_missing_prints_nothing(self):
        # (an empty line once read as "depots to download" in install.sh)
        with tempfile.TemporaryDirectory() as d:
            for depot in [games.BASE_DEPOT, 976738]:
                for m in games.DEPOTS[depot][2]:
                    (Path(d) / m).parent.mkdir(parents=True, exist_ok=True)
                    (Path(d) / m).touch()
            self.assertEqual(self.run_games("missing", d, "halo1"), "")
            self.assertEqual(self.run_games("missing", d, "halo1", "halo3"), "976739\n")
            self.assertEqual(self.run_games("installed", d), "")

    def test_depots(self):
        self.assertEqual(games.needed({"halo3"}), [976731, 976738, 976739])
        self.assertEqual(games.needed({"halo1"}), [])
        self.assertEqual(games.needed({"halo2", "halo3"}), [976731, 976735, 976738, 976739])
        # an add-on (Halo 2's multiplayer maps) comes right after its game, and only with it
        self.assertEqual(games.needed({"halo2", "halo2-mp", "halo3"}), [976731, 976735, 976736, 976738, 976739])
        self.assertEqual(games.needed({"halo2-mp", "halo3"}), [976731, 976738, 976739])
        self.assertEqual(games.needed({"halo3", "halo3-odst"}), [976731, 976738, 976739, 1064222])  # (ODST, when ticked)

    def test_play_with_and_config_kept(self):
        with tempfile.TemporaryDirectory() as d:
            config = Path(d) / "config.json"
            games.write_config(config, d, "existing", {"halo1", "halo3"})
            games.play_with(config, "halo3", "reclaimer")
            with self.assertRaises(ValueError):
                games.play_with(config, "halo1", "reclaimer")
            games.write_config(config, d, "existing", {"halo3", "halo2-mp", "halo2"})
            c = json.loads(config.read_text())
            self.assertEqual(c["playWith"], {"halo3": "reclaimer"})
            self.assertEqual(c["wanted"], ["halo2", "halo2-mp", "halo3"])

    def test_catalogue(self):
        self.assertEqual(games.PLAY_WITH, {"halo1": ["chupathingyce"], "halo2": ["gravemind"], "halo3": ["reclaimer"],
                                         "haloreach": ["renaissance"]})
        self.assertEqual(games.GAMES, {"halo1": [], "halo2": [976735], "halo3": [976738, 976739],
                                     "haloreach": [1064225, 1064226]})
        self.assertEqual(self.run_games("mods", "halo1"), "chupathingyce mac - ChupathingyCE\n")
        self.assertEqual(self.run_games("mods", "halo3", "halo1"), "chupathingyce mac - ChupathingyCE\n"
                         "reclaimer wine mesa,spinfix,loopback,presets Project Reclaimer\n")
        self.assertEqual(self.run_games("mods", "halo2"), "gravemind wine - Project Gravemind\n")
        self.assertEqual(self.run_games("mods"), "")
        self.assertIn("haloreach", [g["id"] for g in games.CATALOGUE["games"]])
        self.assertTrue(games.runs_here("renaissance"))
        self.assertEqual(games.needed({"haloreach", "halo3"}), [976731, 976738, 976739, 1064225, 1064226])
        self.assertEqual(self.run_games("mods", "haloreach"),
                         "renaissance wine msruntimes,chromehoist,presets Reach Renaissance\n")


class Release(http.server.BaseHTTPRequestHandler):
    """A stand-in for GitHub: /api/<owner>/<repo>/releases/latest, then its files, with byte ranges. files: path ->
    bytes; ranges: the Range headers it was asked for; cut: serve only this many bytes of the next file bigger than
    that, then hang up."""
    files, ranges, cut = {}, [], None

    def do_GET(self):
        body = self.files.get(self.path)
        if body is None:
            self.send_error(404)
            return
        start = 0
        if self.headers.get("Range"):
            Release.ranges.append(self.headers["Range"])
            start = int(self.headers["Range"].split("=")[1].rstrip("-"))
        self.send_response(206 if start else 200)
        self.send_header("Content-Length", str(len(body) - start))
        self.end_headers()
        if Release.cut is not None and len(body) - start > Release.cut:
            self.wfile.write(body[start:start + Release.cut])
            Release.cut = None
            return  # (the client sees a short read)
        self.wfile.write(body[start:])

    def log_message(self, *args):
        pass


class Mods(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Release)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.url = f"http://127.0.0.1:{cls.server.server_address[1]}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.home = Path(self.tmp.name)
        Release.files, Release.ranges, Release.cut = {}, [], None

    def tearDown(self):
        self.tmp.cleanup()

    def publish(self, mod, asset, body, sums=None):
        """The mod's latest release on the stand-in: the asset and its checksum file, or for a mod without one, the
        asset's SHA-256 as GitHub lists it (its digest)."""
        source = games.MODS[mod]["source"]
        base = f"/files/{source['github']}"
        digest = sums or hashlib.sha256(body).hexdigest()
        Release.files[f"{base}/{asset}"] = body
        names = [asset]
        if "sums" in source:
            Release.files[f"{base}/{source['sums']}"] = f"{digest}  {asset}\n".encode()
            names.append(source["sums"])
        Release.files[f"/api/{source['github']}/releases/latest"] = json.dumps({"tag_name": "v9.9", "assets": [
            {"name": n, "size": len(Release.files[f"{base}/{n}"]), "browser_download_url": f"{self.url}{base}/{n}",
             **({} if "sums" in source else {"digest": f"sha256:{digest}"})}
            for n in names]}).encode()

    def mods(self, *args):
        env = dict(os.environ, DROPPOD_HOME=str(self.home), DROPPOD_RELEASES=f"{self.url}/api")
        p = subprocess.run([sys.executable, "-I", "-B", str(SCRIPTS / "mods.py"), *args], capture_output=True,
                           text=True, env=env)
        return p.stdout.strip().splitlines()

    def test_which(self):
        self.assertEqual(self.mods("which", "halo1"), ["chupathingyce mac"])
        self.assertEqual(self.mods("which", "halo3", "chupathingyce"), ["reclaimer wine"])  # (not one of Halo 3's)
        self.assertEqual(self.mods("which", "halo2"), ["gravemind wine"])
        self.assertEqual(self.mods("which", "halo4"), [])

    def test_get_a_program_and_keep_the_rest(self):
        folder = self.home / "mods" / "reclaimer"
        folder.mkdir(parents=True)
        (folder / "project-reclaimer-v0.9.1.exe").write_bytes(b"old")
        (folder / "opengl32.dll").write_bytes(b"mesa")
        self.publish("reclaimer", "project-reclaimer-client-v0.9.22.exe", b"MZ" + b"x" * 5000)
        out = self.mods("get", "reclaimer")
        self.assertEqual(out[-1], "ok already")  # (an installed mod updates itself)
        (folder / "project-reclaimer-v0.9.1.exe").unlink()
        (folder / "project-reclaimer-v0.9.1.exe").write_bytes(b"old")
        env_repair = dict(os.environ, DROPPOD_HOME=str(self.home), DROPPOD_RELEASES=f"{self.url}/api", DROPPOD_REPAIR="1")
        out = subprocess.run([sys.executable, "-I", "-B", str(SCRIPTS / "mods.py"), "get", "reclaimer"],
                             capture_output=True, text=True, env=env_repair).stdout.splitlines()
        self.assertEqual(out[-1], "ok")
        self.assertIn("status: Downloading Project Reclaimer 9.9 (0 MB)…", out)
        self.assertIn("progress: 100", out)
        self.assertEqual(sorted(p.name for p in folder.iterdir()), ["opengl32.dll", "project-reclaimer-client-v0.9.22.exe"])
        self.assertEqual(self.mods("program", "reclaimer"), [str(folder / "project-reclaimer-client-v0.9.22.exe")])

    def test_resume_and_checksum(self):
        body = b"MZ" + os.urandom(300_000)
        self.publish("reclaimer", "project-reclaimer-client-v1.0.0.exe", body)
        Release.cut = 100_000  # the first try stops partway
        out = self.mods("get", "reclaimer")
        self.assertTrue(out[-1].startswith("error: "), out)
        part = self.home / "downloads" / "project-reclaimer-client-v1.0.0.exe.part"
        self.assertEqual(part.stat().st_size, 100_000)
        self.assertEqual(self.mods("get", "reclaimer")[-1], "ok")  # the next picks up where it stopped
        self.assertEqual(Release.ranges, ["bytes=100000-"])
        self.assertEqual((self.home / "mods" / "reclaimer" / "project-reclaimer-client-v1.0.0.exe").read_bytes(), body)
        self.assertFalse(part.exists())
        # a download that doesn't match the release's checksum is never kept
        self.publish("reclaimer", "project-reclaimer-client-v1.0.1.exe", body, sums="0" * 64)
        env = dict(os.environ, DROPPOD_HOME=str(self.home), DROPPOD_RELEASES=f"{self.url}/api", DROPPOD_REPAIR="1")
        out = subprocess.run([sys.executable, "-I", "-B", str(SCRIPTS / "mods.py"), "get", "reclaimer"],
                             capture_output=True, text=True, env=env).stdout.splitlines()
        self.assertEqual(out[-1], "error: The Project Reclaimer download didn't match its published checksum.")
        self.assertEqual(self.mods("program", "reclaimer"),
                         [str(self.home / "mods" / "reclaimer" / "project-reclaimer-client-v1.0.0.exe")])

    def test_get_a_mac_app(self):
        z = io.BytesIO()
        with zipfile.ZipFile(z, "w") as f:
            f.writestr("ChupathingyCE.app/Contents/MacOS/halo", "#!/bin/sh\n")
            f.writestr("LICENSE.txt", "GPL")
        self.publish("chupathingyce", "chupathingyce-macos-release.zip", z.getvalue())
        self.assertEqual(self.mods("get", "chupathingyce")[-1], "ok")
        app = self.home / "mods" / "chupathingyce" / "ChupathingyCE.app"
        self.assertTrue((app / "Contents" / "MacOS" / "halo").is_file())
        self.assertEqual(self.mods("program", "chupathingyce"), [str(app)])
        self.assertEqual(list((self.home / "downloads").iterdir()), [])

    def test_get_a_folder_checked_against_github(self):
        z = io.BytesIO()
        with zipfile.ZipFile(z, "w") as f:
            f.writestr("h2gravemind.exe", "MZ")
            f.writestr("LEGAL.md", "terms")
        self.publish("gravemind", "h2gravemind.zip", z.getvalue(), sums="0" * 64)
        self.assertEqual(self.mods("get", "gravemind")[-1],
                         "error: The Project Gravemind download didn't match its published checksum.")
        self.publish("gravemind", "h2gravemind.zip", z.getvalue())
        self.assertEqual(self.mods("get", "gravemind")[-1], "ok")
        folder = self.home / "mods" / "gravemind"
        self.assertEqual(sorted(p.name for p in folder.iterdir()), ["LEGAL.md", "h2gravemind.exe"])
        self.assertEqual(self.mods("program", "gravemind"), [str(folder / "h2gravemind.exe")])
        script = "\n".join(self.mods("launch", "gravemind"))
        out = subprocess.run(["/bin/zsh", "-c", script + "\nprint -r -- $EXE ${#ARGS} x${MOD_OVERRIDES}x"],
                             capture_output=True, text=True).stdout.strip()
        self.assertEqual(out, "h2gravemind.exe 0 xx")

    def test_a_mod_that_stops_under_wine(self):
        # Keep the generic guard covered without declaring a real, supported mod unavailable.
        scripts = self.home / "scripts"
        shutil.copytree(SCRIPTS, scripts)
        catalogue = json.loads((scripts / "mods.json").read_text())
        why = "This test mod stops under Wine."
        next(m for m in catalogue["mods"] if m["id"] == "renaissance")["notUnderWine"] = why
        (scripts / "mods.json").write_text(json.dumps(catalogue))
        env = dict(os.environ, DROPPOD_HOME=str(self.home), DROPPOD_PLATFORM="mac")
        for command in ("get", "launch"):
            p = subprocess.run([sys.executable, "-I", "-B", str(scripts / "mods.py"), command, "renaissance"],
                               capture_output=True, text=True, env=env)
            self.assertEqual(p.returncode, 1)
            self.assertEqual((p.stdout + p.stderr).strip(), f"error: {why}")

    def test_unpack_windows_paths(self):
        # Reach Renaissance's zip writes its paths with backslashes; ditto would keep them in the file names
        z = Path(self.tmp.name) / "w.zip"
        with zipfile.ZipFile(z, "w") as f:
            f.writestr("ReachRenaissance\\Reach Renaissance.exe", "MZ")
            f.writestr("ReachRenaissance\\bin\\rr-browser.exe", "MZ")
        mods.unpack(z, Path(self.tmp.name) / "out")
        self.assertTrue((Path(self.tmp.name) / "out" / "ReachRenaissance" / "bin" / "rr-browser.exe").is_file())
        self.assertTrue((Path(self.tmp.name) / "out" / "ReachRenaissance" / "Reach Renaissance.exe").is_file())
        with zipfile.ZipFile(z, "w") as f:
            f.writestr("ReachRenaissance\\..\\..\\outside.txt", "x")
        with self.assertRaisesRegex(RuntimeError, "outside its folder"):
            mods.unpack(z, Path(self.tmp.name) / "out2")
        self.assertFalse((Path(self.tmp.name) / "outside.txt").exists())

    def test_space_is_checked_first(self):
        with self.assertRaisesRegex(RuntimeError, "there isn't enough free space for X"):
            mods.download(f"{self.url}/nothing", self.home / "x", "0" * 64, "X", size=10 ** 18)

    def test_launch(self):
        folder = self.home / "mods" / "reclaimer"
        folder.mkdir(parents=True)
        (folder / "project-reclaimer-client-v0.9.22.exe").write_bytes(b"MZ")
        script = "\n".join(self.mods("launch", "reclaimer"))
        env = dict(os.environ, LP_NUM_THREADS="2")  # (what the player set wins)
        env.pop("GALLIUM_DRIVER", None)
        out = subprocess.run(["/bin/zsh", "-c", script + "\nprint -r -- $WINEPREFIX $EXE ${#ARGS} $GALLIUM_DRIVER "
                              "$LP_NUM_THREADS $MOD_OVERRIDES; print -r -- ${ARGS[3]}"],
                             capture_output=True, text=True, env=env).stdout.splitlines()
        self.assertEqual(out[0], f"{self.home / 'prefixes' / 'reclaimer'} project-reclaimer-client-v0.9.22.exe 3 "
                                 "llvmpipe 2 opengl32=n,b;xaudio2_9=d")
        self.assertEqual(out[1], r"C:\Program Files (x86)\Steam\steamapps\common\Halo The Master Chief Collection")

    def test_launch_reach(self):
        folder = self.home / "mods/renaissance/ReachRenaissance"
        folder.mkdir(parents=True)
        (folder / "Reach Renaissance.exe").write_bytes(b"MZ")
        script = "\n".join(self.mods("launch", "renaissance"))
        env = dict(os.environ)
        env.pop("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", None)
        out = subprocess.run(["/bin/zsh", "-c", script +
                              '\nprint -r -- "$MOD_DIR|$EXE|${#ARGS}|$MOD_OVERRIDES|$WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"'],
                             capture_output=True, text=True, env=env, check=True).stdout.strip()
        self.assertEqual(out, "ReachRenaissance|Reach Renaissance.exe|0|"
                         "vcruntime140,vcruntime140_1,msvcp140,msvcp140_1,msvcp140_2,concrt140,d3dcompiler_47=n,b;"
                         "xaudio2_9=d|--no-sandbox")

    def test_chromehoist_restored_after_webview_update(self):
        edge = self.home / "prefixes/renaissance/drive_c/Program Files (x86)/Microsoft/EdgeWebView/Application"
        ours = self.home / "chromehoist/msedge_elf.dll"
        ours.parent.mkdir()
        ours.write_bytes(b"our proxy")
        original = b"MZ" + b"x" * 1_000_000
        for version in ("99.0.1.0", "156.0.1.0"):
            runtime = edge / version
            runtime.mkdir(parents=True)
            (runtime / "msedge_elf.dll").write_bytes(original)
        newest = edge / "156.0.1.0"
        self.assertIn("chromehoist installed", self.mods("prepare", "renaissance")[0])
        self.assertEqual((newest / "msedge_elf.dll").read_bytes(), ours.read_bytes())
        self.assertEqual((newest / "msedge_elf_orig.dll").read_bytes(), original)
        self.assertEqual((edge / "99.0.1.0/msedge_elf.dll").read_bytes(), original)
        self.assertEqual(self.mods("prepare", "renaissance"), [])
        updated = edge / "157.0.1.0"
        updated.mkdir()
        (updated / "msedge_elf.dll").write_bytes(original + b"update")
        self.mods("prepare", "renaissance")
        self.assertEqual((updated / "msedge_elf_orig.dll").read_bytes(), original + b"update")
        # A new proxy build replaces only our previous proxy, keeping the forwarding target intact.
        ours.write_bytes(b"new proxy")
        self.mods("prepare", "renaissance")
        self.assertEqual((updated / "msedge_elf.dll").read_bytes(), b"new proxy")
        self.assertEqual((updated / "msedge_elf_orig.dll").read_bytes(), original + b"update")
        self.assertFalse((updated / "msedge_elf.dll.new").exists())

    def test_chromehoist_refuses_to_overwrite_without_original(self):
        ours = self.home / "chromehoist/msedge_elf.dll"
        ours.parent.mkdir()
        ours.write_bytes(b"proxy")
        runtime = self.home / "prefixes/renaissance/drive_c/Program Files (x86)/Microsoft/EdgeWebView/Application/156.0.1.0"
        runtime.mkdir(parents=True)
        (runtime / "msedge_elf.dll").write_bytes(b"unrecognized small DLL")
        p = subprocess.run([sys.executable, "-I", "-B", str(SCRIPTS / "mods.py"), "prepare", "renaissance"],
                           env=dict(os.environ, DROPPOD_HOME=str(self.home)), capture_output=True, text=True)
        self.assertEqual(p.returncode, 1)
        self.assertIn("no original msedge_elf.dll", p.stderr)
        self.assertEqual((runtime / "msedge_elf.dll").read_bytes(), b"unrecognized small DLL")


class ReachAssets(unittest.TestCase):
    def test_watch_reach_ignores_other_installs_and_runtime_updater(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as other:
            home = Path(d).resolve()  # lsof reports macOS's canonical /private/var path
            # Simulate a lingering WebView2 updater in this install and Reach running in another install.
            processes = [subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)", name], cwd=cwd)
                         for name, cwd in (("MicrosoftEdgeUpdate.exe", d),
                                           (r"ReachRenaissance\bin\host-probe.exe", other))]
            cmd = [sys.executable, "-I", "-B", str(SCRIPTS / "tools.py"), "game-running", "renaissance"]
            env = dict(os.environ, DROPPOD_HOME=str(home))
            try:
                self.assertEqual(subprocess.check_output(cmd, env=env, text=True).strip(), "stopped")
                processes.append(subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)",
                                                   r"ReachRenaissance\bin\host-probe.exe"], cwd=home))
                self.assertEqual(subprocess.check_output(cmd, env=env, text=True).strip(), "running")
            finally:
                for process in processes:
                    process.terminate()
                    process.wait(timeout=5)

    def test_restore_graphics_with_only_reach_installed(self):
        with tempfile.TemporaryDirectory() as d:
            home = Path(d)
            shutil.copytree(SCRIPTS, home / "scripts")
            prefix = home / "prefixes/renaissance"
            (prefix / "drive_c/users/player").mkdir(parents=True)
            (prefix / "system.reg").touch()
            out = subprocess.run([sys.executable, "-I", "-B", str(home / "scripts/tools.py"), "reset-graphics"],
                                 env=dict(os.environ, DROPPOD_HOME=d), capture_output=True, text=True, check=True)
            self.assertEqual(out.stdout.strip(), "ok")
            settings = prefix / "drive_c/users/player/AppData/Local/ReachRenaissance/settings.txt"
            self.assertEqual(settings.read_text(), "fps=60\nvsync=on\n")

    def test_pinned_proxy_and_export_table(self):
        repo = SCRIPTS.parent
        pin = re.search(r"^CHROMEHOIST_SHA=([0-9a-f]{64})", (repo / "install.sh").read_text(), re.M)[1]
        dll = repo / "chromehoist/msedge_elf.dll"
        self.assertEqual(hashlib.sha256(dll.read_bytes()).hexdigest(), pin)
        out = subprocess.run([sys.executable, "-I", "-B", str(repo / "chromehoist/exports.py"), str(dll)],
                             capture_output=True, text=True, check=True).stdout
        definitions = (repo / "chromehoist/msedge_elf.def").read_text()
        self.assertEqual([s for s in out.splitlines() if not s.startswith(";")],
                         [s for s in definitions.splitlines() if not s.startswith(";")])
        self.assertEqual(sum("=" in s for s in out.splitlines()), 68)

    def test_presets_only_seed_missing_keys(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "prefixes/renaissance/drive_c/users/player/AppData/Local/ReachRenaissance/settings.txt"
            path.parent.mkdir(parents=True)
            path.write_text("volume=12\nfps=144")
            cmd = [sys.executable, "-I", "-B", str(SCRIPTS / "presets.py"), "renaissance"]
            env = dict(os.environ, DROPPOD_HOME=d)
            subprocess.run(cmd, env=env, check=True, capture_output=True)
            self.assertEqual(path.read_text(), "volume=12\nfps=144\nvsync=on\n")
            subprocess.run(cmd, env=env, check=True, capture_output=True)
            self.assertEqual(path.read_text(), "volume=12\nfps=144\nvsync=on\n")
            path.unlink()
            subprocess.run(cmd, env=env, check=True, capture_output=True)
            self.assertEqual(path.read_text(), "fps=60\nvsync=on\n")


class CrashReports(unittest.TestCase):
    def test_only_dumps_and_screenshots_go(self):
        with tempfile.TemporaryDirectory() as d:
            home, mcc = Path(d) / "home", Path(d) / "MCC"
            (mcc / "crash_report").mkdir(parents=True)
            home.mkdir()
            (mcc / "crash_report" / "minidump.dmp").write_bytes(b"x" * 1000)
            (mcc / "crash_report" / "crash_screenshot.bmp").write_bytes(b"y" * 10)
            (mcc / "crash_report" / "notes.txt").write_text("stays")
            (home / "config.json").write_text(json.dumps({"gameFiles": str(mcc)}))
            env = dict(os.environ, DROPPOD_HOME=str(home))
            out = subprocess.run([sys.executable, "-I", "-B", str(SCRIPTS / "tools.py"), "clean-crash-reports"],
                                 capture_output=True, text=True, env=env).stdout.strip()
            self.assertEqual(out, "ok 1010")
            self.assertEqual(sorted(p.name for p in (mcc / "crash_report").iterdir()), ["notes.txt"])

    def test_repair_with_no_game_from_mcc_checks_nothing(self):
        # (Halo CE alone: no depots to check, so no 16 GB download of the shared files)
        with tempfile.TemporaryDirectory() as d:
            home, files = Path(d) / "home", Path(d) / "Game Files"
            home.mkdir()
            files.mkdir()
            (home / "config.json").write_text(json.dumps({"gameFiles": str(files)}))
            out = subprocess.run([sys.executable, "-I", "-B", str(SCRIPTS / "tools.py"), "repair"],
                                 capture_output=True, text=True, env=dict(os.environ, DROPPOD_HOME=str(home))).stdout
            self.assertEqual(out.strip(), "ok")


if __name__ == "__main__":
    unittest.main(verbosity=1)
