# Changelog

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
