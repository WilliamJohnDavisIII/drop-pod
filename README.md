# Drop Pod

Drop Pod sets up **community-made ways to play the Halo campaigns** on an Apple Silicon Mac, with split-screen
co-op. It's one app that sets everything up from a window, with no Terminal: the community program, the Windows
layer it needs and your own game files, each through a guided setup.

*(Working name. The name is set in one place, the `APP_NAME` file: the app, its folder, the disk image and every
message follow it on the next build.)*

| Game | Plays with | From |
|---|---|---|
| Halo: Combat Evolved | **[ChupathingyCE](https://github.com/ChupathingyCE/chupathingyce)**, a native port: a Mac app of its own | your own original Xbox disc, as a disc image |
| Halo 2 | **[Project Gravemind](https://github.com/BriRoz1/h2gravemind-releases)**, with Halo 2-style menus, co-op campaign for up to 6 over the internet or your network, and split-screen | your Steam copy of Halo: The Master Chief Collection: 16.0 GB shared + 29.1 GB campaign, + 19.7 GB multiplayer maps if you tick them |
| Halo 3 | **[Project Reclaimer](https://github.com/ProjectReclaimer/project-reclaimer-releases)**, with its own menus, Forge and split-screen for up to 4 | your Steam copy of Halo: The Master Chief Collection: 16.0 GB shared + 8.4 GB campaign + 8.7 GB multiplayer |

Project Gravemind and Project Reclaimer run under Wine Staging 11.18 + DXVK-macOS 1.10.3 + MoltenVK 1.4.2 (Direct3D 11 → Vulkan →
Metal), through Rosetta. Drop Pod only downloads, sets up and starts each community program (each then updates
itself); it never includes game files. The games and their mods are listed in one file, the mod catalogue
(`scripts/mods.json`). Drop Pod is forked from the MIT-licensed
[Project Reclaimer Mac Port](https://github.com/mattromano/project-reclaimer-mac-port) by Matt Romano, which did
most of the hard work on the installer, the Steam QR sign-in and Halo 3. See [CREDITS.md](CREDITS.md).

> **Status: not released.** More games come as their community mods run on the Mac.

## What you need

- A Mac with Apple Silicon (M1 or newer) on macOS 14 Sonoma or newer.
- For Halo 2 or Halo 3: Halo: The Master Chief Collection on Steam, and the Steam app on your phone (to scan a
  sign-in code).
- About 3 GB free on your Mac, plus the game files (an external drive works for those): 33 GB for Halo 3, 45 GB for
  Halo 2 (65 GB with its multiplayer maps), 62 GB for both (they share 16 GB).
- Your Mac password, once (for Halo 3).
- For Halo CE: a disc image (`.iso` or `.xiso`) of your own original Halo: Combat Evolved Xbox disc, and about 2 GB
  more on your Mac.

## Installing, step by step

1. **Open the disk image.** Double-click `Drop-Pod.dmg`. A window opens with "Drop Pod", "Applications", a picture
   called "How to open it the first time" and "Read Me First".
2. **Copy the app.** Drag "Drop Pod" onto "Applications". (You can also run it straight from the disk image: it
   copies itself into your Applications folder when setup finishes.)
3. **Open it the first time.** Open your Applications folder and double-click "Drop Pod". macOS says it can't check
   the app, because it isn't from the App Store. Click **Done**. Then:
   1. Open the **Apple menu** > **System Settings**.
   2. Click **Privacy & Security** in the sidebar and scroll down to **Security**.
   3. Next to "Drop Pod was blocked…", click **Open Anyway**.
   4. Click **Open Anyway** again and type your Mac password.

   You only do this once.
4. **Check your Mac.** The window checks for Apple Silicon, macOS 14, free space and Rosetta.
   - If it asks for **Apple's developer tools**, click the button: macOS asks to install them. Click **Install**,
     then **Agree**, and wait about 5 minutes. The window carries on by itself.
   - Rosetta (Apple's Intel translator) is installed during setup if it's missing.

   Click **Continue**.
5. **Choose your games.** All are ticked, each with the mod it plays on and its size. Halo 2 comes with Project
   Gravemind (4 MB); tick **Multiplayer maps** under it for its matches (19.7 GB more; the campaign doesn't need them). Halo 3 comes with Project
   Reclaimer: its program, the graphics library its menus need and the local network addresses it uses. Halo CE
   comes with ChupathingyCE (26 MB) and needs no Steam download: your disc comes in step 9. Click **Continue**; with
   only Halo CE ticked, the button is **Install** and setup goes straight to step 9 (no Wine, no Steam, no
   password).
6. **Choose where the game files are** (when Halo 2 or Halo 3 is ticked).
   - **Download them from Steam**: the default folder is fine, or click **Change…** to choose another, such as
     an external drive.
   - **Use an MCC folder I already have** (for example one copied from a Windows PC): click **Choose…** and select
     the folder with `MCC`, `Data` and the game folders in it. It's used where it is, and only what it lacks is
     downloaded into it. If Drop Pod finds such a folder on your Mac, it picks it for you.

   The window shows how much will be downloaded and whether it fits. Click **Install**.
7. **Approve the password prompt.** macOS asks for your Mac password once (for Rosetta, if it's missing, and
   Project Reclaimer's local network addresses).
8. **Scan the QR code** (only if something needs downloading). When a QR code appears:
   1. Open the **Steam** app on your phone.
   2. Tap the **shield** (Steam Guard) at the bottom.
   3. Tap **Scan a QR code** and point your phone at the code on your Mac.

   The download starts by itself. It can take a few hours; leave the window open (your Mac won't sleep, the
   screen may). If it stops, press **Try Again**: it picks up where it stopped.
9. **Halo CE.** When setup is done, it offers **Set Up Native Halo CE…** to add your disc (see below). You can skip
   it and do it later from the Halo CE tile.
10. **Play.** Click **Open My Games**, then **Play** on a game. While Halo 3 runs, the menu bar, the Dock and
    Cmd-Tab show it as Drop Pod, with Drop Pod's icon. Quitting the game brings you back to the library. You can
    eject the disk image and move it to the Trash.

### The guided setups

**Set up native Halo CE** (from the end of setup, or the Halo CE tile):
1. It explains what the native port is and why it needs your own original Halo: Combat Evolved Xbox disc, as a disc
   image: the port plays the original Xbox game's data, which nobody can legally give you, so Drop Pod never
   downloads it.
2. **Choose Disc Image…** opens a file picker for your `.iso` or `.xiso`. Drop Pod checks it right away: that it's
   an Xbox disc image, and that it has Halo's maps folder.
3. **Set Up** downloads the port, unless setup already did (ChupathingyCE's latest release, 26 MB for 0.8.0c,
   checked against its published checksum)…
4. …copies Halo's maps (about 2 GB) out of your disc image into the port's own folder,
   `~/Library/Application Support/ChupathingyCE/maps`, exactly as the port's own first start would, and checks
   them. The port then starts straight into the game. If the copy fails, it offers to let the port do it itself.

**Set up Project Reclaimer** (the Halo 3 tile, when its program is missing, say after a failed download): it says
what it installs and how big it is (Project Reclaimer 45 MB from its GitHub releases, Mesa 70 MB, and the network
helper with one password prompt), then shows each step, progress and plain errors with **Try Again**. Both setups
come from the catalogue: a mod set up with its game gets this sheet, one that plays from your disc the one above.

### Later

- **Get** on Halo 2's or Halo 3's tile downloads it if you skipped it (the QR code only comes back if Steam forgot this Mac).
- **Settings** (top right):
  - *Game files*: where they are; *Moved your game files?* points Drop Pod at their new folder. *Halo 2
    multiplayer maps* (when Halo 2 is there without them) downloads them later.
  - *Repair game files* deletes crash reports, then checks the files against Steam; *Repair Drop Pod* sets up
    Wine, DXVK and the Play buttons again.
  - *Delete crash reports* (shown when there are some): a crashing game leaves a ~3 GB memory dump in the game
    files folder's `crash_report`, as MCC does. Uninstalling deletes them too.
  - *Force quit the game*, *Restore Halo 3's recommended graphics*, *Use a different Steam account*.
  - *Show the logs*, *Make a report* (a zip on your Desktop, without your Steam account name).
  - *Uninstall Drop Pod*. Your game files stay unless Drop Pod downloaded them into a folder of its own and you
    choose to delete them too. Native Halo CE's own folder (its maps, settings and saves) stays.
- Drop Pod checks for updates of itself (from its GitHub releases, once published) each time it opens. The mods
  update themselves when they start: Project Reclaimer in its own window and Project Gravemind from its menu (the
  library waits for the whole Wine session, so a restart after an update stays "Playing"), ChupathingyCE by
  pointing you to its new release.
  *Repair Drop Pod* downloads each mod's latest release again.

## Where things go

| Path | What |
|---|---|
| `~/Applications/Drop Pod.app` (or `/Applications`) | the app |
| `~/Library/Application Support/Drop Pod/` | `mods/<mod>/` (each mod: `reclaimer/` Project Reclaimer's exe and Mesa, `chupathingyce/` ChupathingyCE.app), `prefixes/<mod>` (each Wine mod's own), `wine/`, `tools/DepotDownloader`, `scripts/`, `logs/`, `config.json` |
| `~/Library/Application Support/Drop Pod/game.noindex/Drop Pod.app` | the game app: the library opens it to start Halo 3, so the Mac shows the game as Drop Pod instead of "wine". Its executable is a launch script; beside it is a copy of Wine's loader, with `Contents/lib` and `Contents/share` linking into Wine. (`.noindex`: Spotlight leaves it out.) The native port runs as its own app, ChupathingyCE. |
| the game files folder you chose | MCC's files (shared, `halo3`) |
| `~/Library/Application Support/ChupathingyCE/` | the native port's own folder: `maps` from your disc, its settings and saves |
| `/Library/LaunchDaemons/local.droppod.loopback.plist`, `/Library/Application Support/droppod/` | the startup task that adds the local addresses Project Reclaimer binds (127.0.0-3.x, 127.3.1-3.x) |

`config.json` holds the game files path (Mac and Wine `Z:` forms), where they came from, the installed and wanted
games, each game's `playWith` choice (a mod's ID), and Wine's path. The Play buttons (`scripts/play.sh <game> [mod]`,
`scripts/run_wine.sh <mod>`) read it each time, so moving the game files needs no reinstall. Wine, the game app,
DepotDownloader and the game files folder are only set up when a chosen game's mod needs them.

## For developers

```
launcher/        the window, in Rust (egui on wgpu: Metal on macOS, Direct3D 12 or Vulkan on Windows and Linux)
  src/core.rs    the mod catalogue (games, mods, Steam depots), config.json, install.sh/steam.py output parsing, QR
                 codes (no drawing; its tests: cargo test)
  src/setup.rs   the setup wizard's state      src/library.rs  the library's state, the guided setups, Settings
  src/screens.rs every screen and sheet        src/list.rs     the lists moved through by keys, mouse or controller
  src/ring.rs    the look (theme.rs: fonts and painting)   art.rs  MCC's tile pictures, read from the game files
  src/input.rs   keys and game controllers (gilrs)          sound.rs  the menu sounds, made in code (rodio)
  src/proc.rs    running the scripts            src/states.rs  --render-states and the "Open Anyway" picture
  assets/fonts/  Michroma and Titillium Web (OFL)           tests/  the QR sample the tests read
APP_NAME         the app's name (the one place to change it)
assets/icon/     the icon: Droppod.icns (in the app, the game app and the disk image), droppod-icon.svg, the 1024 px
                 PNG, and make_icon.py with tools/ (iconkit.py, render.swift) that draw them
install.sh       the installer, one function per step, GUI mode for the window, dry-run mode for testing
scripts/         mods.json (the mod catalogue)  games.py (the catalogue's games and depots, config.json)  mods.py
                 (download a mod: latest release, checksum, resume, space check; how to start it)  steam.py
                 (DepotDownloader + QR relay)  updater.py (the app's own updates)  tools.py (Settings' and the guided
                 setups' commands)  xiso.py (Halo's maps out of an Xbox disc image)  presets.py (Project Reclaimer's
                 graphics)  play.sh  run_wine.sh  game-launch.sh.in (the game app's executable)
                 droppod-loopback.sh + its plist
                 make_app.sh  make_dmg.sh  make_icon.sh
tests/           test_scripts.py: xiso.py on made-up disc images, the catalogue (games.py, mods.py against a stand-in
                 for GitHub), crash-report cleanup, repair
spinfix/         the Reclaimer port's d3d11.dll shim (source + pinned build)
```

### The mod catalogue

`scripts/mods.json` lists the games and the mods they play with, the default first; the app (from inside its bundle)
and the scripts read the same file. Per mod: `source` (a GitHub repo, the release asset to take, by pattern, and the
release's checksum file), `mb`, the MCC `depots` it needs and `mccBuild` ("current": it follows Steam's MCC), `runs`
(`wine`, in `prefixes/<id>`, or `mac`), how it starts (`program` in `mods/<id>/`, `args`, `dllOverrides`, `env`), the
`process` the library watches (for a Wine mod it watches the whole Wine session), `extras` install.sh adds (`mesa`,
`spinfix`, `loopback`, `presets`), `setup` (`with-game`, or `xbox-disc` for the disc import), and the words its tile
and guided setup use. A new Wine mod that needs nothing new is one entry here.

Build and test (Xcode's command line tools and Rust from rustup; `launcher/rust-toolchain.toml` picks the version):

```sh
scripts/make_app.sh test                 # unit tests: launcher's core (and the catalogue), and the scripts
scripts/make_app.sh states /tmp/states   # build "dist/Drop Pod.app" and write a PNG of every screen
scripts/make_dmg.sh                      # dist/Drop-Pod.dmg, plus Drop-Pod.zip + SHA256SUMS for a GitHub release
scripts/make_icon.sh                     # redraw the icon and rebuild assets/icon/Droppod.icns
python3 -I scripts/make_notices.py       # THIRD_PARTY_NOTICES, after changing launcher's crates
```

Dry-run the installer into a scratch folder, using an existing MCC folder, without downloading anything, starting
Wine or asking for a password:

```sh
DROPPOD_HOME=/tmp/droppod-test DROPPOD_DRY_RUN=1 DROPPOD_SOURCE=existing \
  DROPPOD_GAME_FILES="/Volumes/Library/Games/Halo MCC" DROPPOD_GAMES="halo1 halo3" ./install.sh
```

The other options (`DROPPOD_GUI`, `DROPPOD_UPDATE`, `DROPPOD_REPAIR`, `DROPPOD_CACHE`, `DROPPOD_APP_SOURCE`,
`DROPPOD_RESET_GRAPHICS`) are described at the top of `install.sh`. The app finds an existing MCC folder itself
(`DROPPOD_MCC_FROM` overrides), `DROPPOD_NATIVE_CE_DATA` moves native Halo CE's data folder (for tests), and
`DROPPOD_REPO=owner/name scripts/make_app.sh` sets the GitHub repo it updates from (empty until the project is
published). For checking the window: `DROPPOD_TEST_STEPS="wait 1; down; accept; shot name; quit"` plays moves into it
(saving PNGs into `DROPPOD_TEST_SHOTS`; the window doesn't take focus and ignores the mouse), `DROPPOD_TIMING=1`
says when its first frame is on screen, `DROPPOD_BENCH=1` times its frames while the menus move, `DROPPOD_SOUNDS=0`
silences the menu sounds, and `DROPPOD_STAY=1` keeps a copy outside Applications on to the library after setup.

## Licence

Drop Pod is free software under the GNU GPL, version 3 or later (`LICENSE`). Portions from the Project Reclaimer
Mac port are MIT (`LICENSE.reclaimer-mac-port`).

## Credits

Forked from the [Project Reclaimer Mac Port](https://github.com/mattromano/project-reclaimer-mac-port) by Matt
Romano at commit `832b6a5` (its version 1.2.3), under the MIT licence; its notice is kept in
`LICENSE.reclaimer-mac-port`. Thanks to the Wine, DXVK, MoltenVK, Mesa, DepotDownloader, Project Reclaimer and
ChupathingyCE projects: see [CREDITS.md](CREDITS.md).

Halo and Halo: The Master Chief Collection are trademarks of Microsoft. Drop Pod is an unofficial fan project, not
affiliated with Microsoft, 343 Industries, Halo Studios or Valve. It contains no game files.

## Platforms

macOS: works. Windows and Linux: experimental, untested (I only have a Mac). PRs welcome.

On Windows and Linux the launcher window builds and opens (see Releases), but the automatic setup is Mac-only so far: install each mod by its own instructions (links above), then launch MCC from Steam (on Linux, with Proton). Help porting the setup is very welcome.

Drop Pod is a fan project, not affiliated with or endorsed by Microsoft, 343 Industries or Halo Studios.
