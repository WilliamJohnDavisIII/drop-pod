# Developing Drop Pod

How Drop Pod is put together, where it puts things, and how to build and test it. For what it is and how to
install it, see the [README](../README.md).

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
  src/music.rs   each game's title music, faded between tiles  mcc_audio.rs  reads it from the game files (lewton decodes)
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
                 graphics and missing Reach defaults)  play.sh  run_wine.sh  game-launch.sh.in (the game app's executable)
                 droppod-loopback.sh + its plist
                 make_app.sh  make_dmg.sh  make_icon.sh
tests/           test_scripts.py: xiso.py on made-up disc images, the catalogue (games.py, mods.py against a stand-in
                 for GitHub), crash-report cleanup, repair
spinfix/         the Reclaimer port's d3d11.dll shim (source + pinned build)
chromehoist/     the WebView2 menu fix for Reach on Mac (MIT source + pinned build; build.sh uses Zig)
```

### The mod catalogue

`scripts/mods.json` lists the games and the mods they play with, the default first; the app (from inside its bundle)
and the scripts read the same file. Per mod: `source` (a GitHub repo, the release asset to take, by pattern, and the
release's checksum file), `mb`, the MCC `depots` it needs and `mccBuild` ("current": it follows Steam's MCC), `runs`
(`wine`, in `prefixes/<id>`, or `mac`), how it starts (`program` in `mods/<id>/`, `args`, `dllOverrides`, `env`), the
`process` the library watches (for a Wine mod it watches the whole Wine session), `extras` install.sh adds (`mesa`,
`spinfix`, `loopback`, `presets`, `msruntimes`, `chromehoist`), `setup` (`with-game`, or `xbox-disc` for the disc import), and the words its tile
and guided setup use. A new Wine mod that needs nothing new is one entry here.

On Mac, Reach's `msruntimes` extra installs the native Microsoft Visual C++ x64 and WebView2 runtimes into its
own prefix and copies `d3dcompiler_47.dll` from the player's MCC CEF3 files. Game files are checked/downloaded
before mod setup so that compiler is available on a first install. `chromehoist` preserves WebView2's original
`msedge_elf.dll` as `msedge_elf_orig.dll` and installs our forwarding DLL in its version folder; `mods.py prepare`
repeats the check before each launch, including after an Evergreen update. This is specific to Wine's Mac driver.
The WebView2 installer leaves EdgeUpdate running: setup waits for the installer, then checks its files instead of
waiting for the whole Wine session. Reach's library tile likewise watches its own game processes through
`tools.py game-running renaissance`, with working folders checked to exclude separate installs.

To update these runtimes, bump the direct Microsoft URL and SHA-256 together in `install.sh` (`VC_REDIST_PKG`,
`WEBVIEW2_PKG`). Regenerate the proxy's exports from the new runtime and rebuild it:

```sh
ZIG=/path/to/zig chromehoist/build.sh "/path/to/EdgeWebView/Application/<version>/msedge_elf.dll"
```

Update `CHROMEHOIST_SHA` in `install.sh` with that build's checksum. The checked-in DLL is built with Zig 0.13;
its PE timestamp may differ on a rebuild. The MIT source describes the window hoist, SetParent patch and command
line changes. Menus remain a separate popup over the game window's contents, stacked with the game's windows rather
than floating above other apps, and a quit sent to the menus is forwarded to the game; the intro video is skipped under Wine.

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
`DROPPOD_REPO=owner/name scripts/make_app.sh` sets the GitHub repo it updates from (default `WilliamJohnDavisIII/drop-pod`). For checking the window: `DROPPOD_TEST_STEPS="wait 1; down; accept; shot name; quit"` plays moves into it
(saving PNGs into `DROPPOD_TEST_SHOTS`; the window doesn't take focus and ignores the mouse), `DROPPOD_TIMING=1`
says when its first frame is on screen, `DROPPOD_BENCH=1` times its frames while the menus move, `DROPPOD_SOUNDS=0`
silences the menu sounds, and `DROPPOD_STAY=1` keeps a copy outside Applications on to the library after setup.
