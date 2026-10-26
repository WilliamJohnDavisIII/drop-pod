# Changelog

## Unreleased

- Halo: Reach with Reach Renaissance on Apple Silicon Macs: Microsoft's Visual C++ and Edge WebView2 runtimes,
  MCC's own shader compiler, and the MIT chromehoist proxy that makes the WebView2 menus visible under Wine.
  Runtime downloads are pinned to direct Microsoft URLs and SHA-256 checksums.
- Restore the menu fix before each launch when WebView2 has updated, and seed missing Reach graphics settings
  with a 60 fps cap and V-Sync, keeping the player's existing choices.
- Reach's menus no longer float above other apps (they stack with the game's window), and Cmd-Q, the Dock's Quit and
  the window's close button end the game, its menus and its browser whichever of them has the focus, so the tile no
  longer stays on Playing.
- Keep WebView2's background updater from stalling setup or leaving Reach's tile stuck on Playing after it exits.
- Include Reach in setup, download sizes and the library. Linux support is enabled but untested; the Mac-specific
  runtime and menu fixes are installed only on macOS.

## 0.2.0 (2026-10-23)

**Windows and Linux (experimental, untested)**
- A guided setup off the Mac too (`scripts/setup.py`, needs Python 3): the same steps the window shows, with
  DepotDownloader's Windows or Linux build for the game files from your Steam account, and each chosen game's mod.
  Nothing Mac-only (Wine, DXVK, Mesa, spinfix, local network addresses, game apps, graphics presets).
- Play starts the mods with `scripts/play.py`: as they are on Windows, under your system's Wine on Linux (each mod
  in its own prefix, with the game files linked where it looks; it asks you to install Wine if it's missing).
- ChupathingyCE downloads its Windows or Linux build.
- Drop Pod's icon on Windows (exe, taskbar) and Linux (app menu, taskbar).
- The setup checks no longer ask for Apple's developer tools or Rosetta there.

**Wording**
- Text shown on every platform says "computer" off the Mac ("Not on this computer yet", free space, uninstall,
  stop-the-game messages, Show Folder) and drops the Mac-only password and network-setting notes there.
- "Community-made ways to play Halo" instead of "the Halo campaigns" in the disk image's read-me.
- README: "Other ways to play", linking HCEMobile and HaloPad (neither part of Drop Pod).

**Build**
- Release builds work without a strip workaround: only the app is stripped, not the build scripts and macros
  cargo builds for itself.

## 0.1.0 (2026-10-23)

The first public release.

**Games**
- **Halo 3 and ODST** with Project Reclaimer: co-op campaign for up to 16, split-screen, multiplayer for up to 128,
  Forge, ODST's Firefight. Set up with Mesa, the spinfix CPU fix, graphics presets and the local network addresses
  it needs.
- **Halo 2** with Project Gravemind: co-op campaign for up to 6, multiplayer for up to 24, split-screen; the
  multiplayer maps are an optional extra.
- **Halo: Combat Evolved** with ChupathingyCE, the native port: Drop Pod checks your own Xbox disc image and copies
  Halo's maps out of it.
- **Halo: Reach** with Reach Renaissance: listed, and allowed on Windows (it stops itself under Wine, so not on Mac
  or Linux yet).

**The app**
- A guided setup on the Mac: checks, game choice, where the game files go (download from Steam, or an MCC folder
  you already have), one password prompt, Steam sign-in by QR code, resumable downloads.
- A controller-friendly library: Play, Get, guided setups per game, Settings (move or repair game files, repair Drop
  Pod, crash-report cleanup, force quit, graphics reset, Steam account, logs, report, uninstall).
- Self-updates from this repo's GitHub releases; each community project updates itself.
- Windows and Linux: the app builds and opens (experimental, untested); setup there is manual for now.

**Project**
- Forked from Matt Romano's Project Reclaimer Mac Port (MIT, commit `832b6a5`, version 1.2.3).
- Licensed GPL-3.0-or-later, with the ported portions MIT. Full third-party notices for every bundled crate and font.
- GitHub Actions builds macOS, Windows and Linux on every push and attaches the Windows and Linux builds to releases.
