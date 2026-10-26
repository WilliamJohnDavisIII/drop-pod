# Credits and licences

Drop Pod stands on other people's work. Most of it is **downloaded by the installer from its own project's
releases** at install time, checked against a pinned SHA-256, and never bundled in this app. What *is* inside
the app is listed under "In the app".

Drop Pod's full source code: https://github.com/WilliamJohnDavisIII/drop-pod

## In the app

| What | Licence | Notes |
|---|---|---|
| **Drop Pod's window** (`launcher/`, in Rust) | GPL-3.0-or-later (`LICENSE`) | The parts ported from the Reclaimer Mac port's window stay MIT (marked in their files' headers). |
| **Drop Pod's installer and scripts** (`install.sh`, `scripts/`, including `xiso.py`, written for it from the Xbox disc format) | GPL-3.0-or-later (`LICENSE`) | |
| **The Rust crates and fonts the window is built from** (egui, eframe, wgpu, gilrs, rodio, lewton and the crates they use; Michroma and Titillium Web) | Each its own: MIT, Apache-2.0, MPL-2.0, Zlib, BSD, ISC; the fonts SIL OFL 1.1 | Every one, with its licence text, in `THIRD_PARTY_NOTICES`. |
| **Drop Pod's icon** (`assets/icon/`: `make_icon.py` and `tools/` draw it in code) | GPL-3.0-or-later (`LICENSE`) | Original geometry: no fonts, game art or traced shapes. |
| **Project Reclaimer Mac Port** by Matt Romano, [mattromano/project-reclaimer-mac-port](https://github.com/mattromano/project-reclaimer-mac-port) | MIT (`LICENSE.reclaimer-mac-port`) | Drop Pod is forked from it: the window's code, `install.sh`, the updater, the Steam QR sign-in relay, the Tools/Settings scripts, the "Open Anyway" guide, the DMG scripts and the Halo 3 setup (run script, graphics presets, network addresses). |
| **spinfix** `d3d11.dll` (from the Reclaimer Mac port, `spinfix/`) | MIT | Built from `spinfix/spinfix.c`; stops Halo 3's engine thread spinning a CPU core between frames. |
| **chromehoist** `msedge_elf.dll` by Devon Fuller (`chromehoist/`) | MIT (licence text in `chromehoist/chromehoist.c` and `THIRD_PARTY_NOTICES`) | Built from `chromehoist/chromehoist.c`; makes Reach Renaissance's WebView2 menus visible under Wine on macOS. Its exports forward to Microsoft's original DLL, installed separately. |

## Downloaded at install time

| What | Licence | Where from |
|---|---|---|
| **Wine Staging 11.18** (Gcenx's macOS builds) | LGPL-2.1-or-later | [Gcenx/macOS_Wine_builds](https://github.com/Gcenx/macOS_Wine_builds) — source: [winehq.org](https://www.winehq.org) |
| **DXVK-macOS 1.10.3** (async build) | zlib/libpng | [Gcenx/DXVK-macOS](https://github.com/Gcenx/DXVK-macOS), a fork of [doitsujin/dxvk](https://github.com/doitsujin/dxvk) |
| **MoltenVK 1.4.2** | Apache-2.0 | [KhronosGroup/MoltenVK](https://github.com/KhronosGroup/MoltenVK) |
| **Mesa 26.2.4** (`opengl32.dll`, `libgallium_wgl.dll`, for Project Reclaimer's menu) | MIT | [pal1000/mesa-dist-win](https://github.com/pal1000/mesa-dist-win) — source: [mesa3d.org](https://www.mesa3d.org) |
| **DepotDownloader 3.4.0** | GPL-2.0 | [SteamRE/DepotDownloader](https://github.com/SteamRE/DepotDownloader). Run as a separate program; Drop Pod only reads its output. |
| **Project Reclaimer** (runs Halo 3) | Its authors' terms (closed source) | [ProjectReclaimer/project-reclaimer-releases](https://github.com/ProjectReclaimer/project-reclaimer-releases), the latest release, checked against its `SHA256SUMS.txt`. Never bundled. |
| **Project Gravemind** (runs Halo 2) | Its authors' terms (closed source) | [BriRoz1/h2gravemind-releases](https://github.com/BriRoz1/h2gravemind-releases), the latest release, checked against the SHA-256 on its release page. Never bundled. |
| **Reach Renaissance** (runs Halo: Reach) | Its authors' terms (closed source) | [NETF1ix/reach-renaissance-releases](https://github.com/NETF1ix/reach-renaissance-releases), the latest release, checked against its `SHA256SUMS.txt`. Never bundled. |
| **Microsoft Visual C++ x64 Runtime and Edge WebView2 Runtime** (for Reach on Mac) | Microsoft's terms, included in their installers | Downloaded directly from Microsoft; URLs and SHA-256 checksums pinned in `install.sh`. Never bundled. |
| **ChupathingyCE** (native Halo CE) | GPL-3.0 | [ChupathingyCE/chupathingyce](https://github.com/ChupathingyCE/chupathingyce), the latest release, only when you choose Halo CE. Drop Pod copies Halo's maps from your own Xbox disc image into its folder, as its own first start would. Its source is at that link. |

## Your games

**Halo: The Master Chief Collection** comes from your own Steam account (or an MCC folder you already have),
under your own Steam licence. Drop Pod never includes or redistributes any of its files; it only shows MCC's own
tile pictures and plays each game's menu music from your game files folder. The same goes for your Halo: Combat Evolved Xbox disc, for native
Halo CE: Drop Pod reads the disc image you choose and never downloads or shares game data.

For Reach's menus, Drop Pod copies `Engine/Binaries/ThirdParty/CEF3/Win64/d3dcompiler_47.dll` from your own MCC
files into Reach's Wine prefix. This Microsoft shader compiler remains under Microsoft's terms and is never bundled.

Halo, Halo: The Master Chief Collection and related marks are trademarks of Microsoft. Steam is a trademark of
Valve. Drop Pod is an unofficial fan project, not affiliated with or endorsed by Microsoft, 343 Industries, Halo
Studios, Valve, or the Project Gravemind, Project Reclaimer or ChupathingyCE projects.
