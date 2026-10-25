#!/bin/bash
# Drop Pod (the name is set in APP_NAME): sets up community mods of the Halo campaigns on an Apple Silicon Mac. The
# mods, their games and what each needs come from the catalogue (scripts/mods.json, through games.py and mods.py):
# Halo 2 runs on Project Gravemind, Halo 3 on Project Reclaimer and Reach on Reach Renaissance, from the player's own Steam copy of Halo: The Master
# Chief Collection, each under Wine Staging + DXVK-macOS + MoltenVK in its own prefix; Halo CE on ChupathingyCE, a Mac app that plays from the player's
# own Xbox disc (imported from the library). Only what the chosen games' mods need is installed: Halo CE alone needs
# no Wine and no Steam download.
# The app's window runs this (DROPPOD_GUI=1) and shows its steps; it also works from Terminal. Safe to re-run: finished
# steps are skipped. Forked from the Project Reclaimer Mac port's install.sh (MIT, see LICENSE.reclaimer-mac-port).
#
# Everything lives in ~/Library/Application Support/<name> (DROPPOD_HOME), except the game files (DROPPOD_GAME_FILES).
# Options (environment variables; the window sets them, re-runs fall back to config.json):
#   DROPPOD_GAMES="halo1 halo2 halo3 haloreach"  the games wanted; each brings its mod (the first in mods.json) and what it needs:
#                               halo3 Project Reclaimer (its program, Mesa, spinfix, the network helper) and Halo 3's
#                               campaign and multiplayer depots; halo2 Project Gravemind and Halo 2's campaign depot
#                               (halo2-mp with it: its multiplayer maps' depot, optional); halo1 ChupathingyCE;
#                               haloreach Reach Renaissance (Microsoft runtimes and the chromehoist menu fix).
#   DROPPOD_GAME_FILES=<dir>     where the game files are or go (default: <home>/Game Files)
#   DROPPOD_SOURCE=steam|existing   download them from Steam, or use an MCC folder the player already has, in place
#                               (only the depots it lacks are downloaded into it)
#   DROPPOD_GUI=1                no Terminal questions; macOS's password dialog; QR code and progress lines
#   DROPPOD_UPDATE=1             refresh an install (run by updater.py after the app updated): no game downloads
#   DROPPOD_REPAIR=1             redo every step's files (Settings > Repair); prefixes and game files stay
#   DROPPOD_DRY_RUN=1            print what would be downloaded, run in Wine, or done as admin, without doing it
#   DROPPOD_CACHE=<dir>          reuse already-downloaded archives from <dir> (kept afterwards)
#   DROPPOD_APP_SOURCE=<.app>    the app the player opened: copied into ~/Applications when it isn't installed there
#   DROPPOD_RESET_GRAPHICS=1     restore tuned graphics settings (Reach only fills in missing keys)
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
NAME=$(cat "$HERE/APP_NAME" 2>/dev/null || echo "Drop Pod")  # the app's name: change it in APP_NAME only
BASE=${DROPPOD_HOME:-$HOME/Library/Application Support/$NAME}
CACHE=${DROPPOD_CACHE:-$BASE/downloads}
CONFIG="$BASE/config.json"
WINE_DIR="$BASE/wine/Wine Staging.app/Contents/Resources/wine"
WINE_BIN="$WINE_DIR/bin"
STEAM_REL="drive_c/Program Files (x86)/Steam"
MCC_REL="$STEAM_REL/steamapps/common/Halo The Master Chief Collection"
APPS=${DROPPOD_APPS_DIR:-$HOME/Applications}
GUI=${DROPPOD_GUI:-}
UPDATE=${DROPPOD_UPDATE:-}
REPAIR=${DROPPOD_REPAIR:-}
DRY=${DROPPOD_DRY_RUN:-}

# name|url|sha256 (the same pins as the Project Reclaimer Mac port)
WINE_PKG="wine-staging-11.18-osx64.tar.xz|https://github.com/Gcenx/macOS_Wine_builds/releases/download/11.18/wine-staging-11.18-osx64.tar.xz|b63704b91af269bc026a87f12bd297c4a50caaf570c322e600b6621ef918f127"
DXVK_PKG="dxvk-macOS-async-v1.10.3-20230507-repack.tar.gz|https://github.com/Gcenx/DXVK-macOS/releases/download/v1.10.3-20230507-repack/dxvk-macOS-async-v1.10.3-20230507-repack.tar.gz|acd1520ad105d8ef124a09c8e11a259a5dc8bdc565ad18e0e52693f9807b2477"
MOLTENVK_PKG="MoltenVK-macos-1.4.2.tar|https://github.com/KhronosGroup/MoltenVK/releases/download/v1.4.2/MoltenVK-macos.tar|f95765a6229cb7b915990a2890ce12ebe36a730b021545d3d52ae69ce4c4024e"
MESA_PKG="mesa3d-26.2.4-release-msvc.7z|https://github.com/pal1000/mesa-dist-win/releases/download/26.2.4/mesa3d-26.2.4-release-msvc.7z|351fc8c8b695878ffb3eaa044b3ead08672a48b1a045e3c3e3975811df0f6695"
DEPOT_PKG="DepotDownloader-macos-arm64.zip|https://github.com/SteamRE/DepotDownloader/releases/download/DepotDownloader_3.4.0/DepotDownloader-macos-arm64.zip|60e80c7c496f3f9a079cd3c62036b35d088c27bc0149baf38f009eb57a52f6a5"
# Direct, version-pinned Microsoft downloads; bump the URL and checksum together when updating the runtimes.
VC_REDIST_PKG="vc_redist.x64.exe|https://download.visualstudio.microsoft.com/download/pr/990c720a-46ce-4dc7-87b9-c2eb615d34b9/CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B/VC_redist.x64.exe|cc0ff0eb1dc3f5188ae6300faef32bf5beeba4bdd6e8e445a9184072096b713b"
WEBVIEW2_PKG="MicrosoftEdgeWebView2RuntimeInstallerX64.exe|https://msedge.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/d6c50665-06a3-4ebe-8bde-7e408a78f093/MicrosoftEdgeWebView2RuntimeInstallerX64.exe|9b0142e22e8daa92855d746ae4d1e9eeef561ccaf67d1dd452165c4af3df3191"
DXVK_DIR=dxvk-macOS-async-v1.10.3-20230507-repack
SPINFIX_SHA=0ec56a555b7b420c381f7cf5010719c86c3626c3c99efcb376981089f5d4f847  # spinfix/d3d11.dll (built from spinfix/spinfix.c)
CHROMEHOIST_SHA=2cae00421bfa39826c97a077213f13b9c4e435bb9c52390e4366c926d86f4bf1  # chromehoist/msedge_elf.dll
# give up on a stalled connection instead of hanging (the window waits for this)
CURL_LIMITS="--connect-timeout 20 --speed-limit 1024 --speed-time 60"
RUNTIME_GB=3  # Wine, the prefixes, the mods and the downloads while they unpack

exec 3>&1  # the real output, for error lines from inside $(...) (the window reads them)
STEP=starting
bold() { STEP=$*; printf '\n\033[1m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }
dry() { printf '    dry-run: %s\n' "$*"; }
die() {
  printf '\n\033[31mError: %s\033[0m\n' "$*" >&2
  [ -n "$GUI" ] && printf 'error: %s\n' "$*" >&3  # (the window shows this line)
  exit 1
}

fetch() {  # fetch "name|url|sha256" -> path of the verified file in $CACHE ($DRY: says so, prints the would-be path)
  local name=${1%%|*} rest=${1#*|}; local url=${rest%%|*} sum=${rest##*|} f="$CACHE/${1%%|*}"
  if [ -n "$DRY" ] && { [ ! -f "$f" ] || [ "$(shasum -a 256 "$f" | cut -d' ' -f1)" != "$sum" ]; }; then
    dry "download $name from $url (sha256 $sum)" >&2
    printf '%s' "$f"; return 0
  fi
  if [ ! -f "$f" ] || [ "$(shasum -a 256 "$f" | cut -d' ' -f1)" != "$sum" ]; then
    note "downloading $name" >&2
    curl -fL $CURL_LIMITS --progress-bar -o "$f.part" "$url" ||
      die "Couldn't download $name. Check your internet connection and try again."
    mv "$f.part" "$f"
  fi
  [ "$(shasum -a 256 "$f" | cut -d' ' -f1)" = "$sum" ] ||
    { rm -f "$f"; die "The download of $name was damaged. Press Try Again to download it again."; }
  printf '%s' "$f"
}

as_admin() {  # as_admin "<shell command>" "<why>": sudo in Terminal; macOS's password dialog from the window
  if [ -n "$DRY" ]; then dry "admin (one password prompt): $1"; return 0; fi
  if [ -n "$UPDATE" ] || [ -n "$GUI" ]; then
    local cmd=${1//\\/\\\\}; cmd=${cmd//\"/\\\"}
    /usr/bin/osascript -e "do shell script \"$cmd\" with prompt \"${2:-$NAME is changing a system setting.}\" with administrator privileges" >/dev/null
  else
    sudo /bin/sh -c "$1" </dev/tty
  fi
}

wine_run() {  # wine_run <prefix> [--no-wait] <wine args...>: normally wait for the prefix's wineserver too
  local pfx=$1 wait=1; shift
  if [ "${1:-}" = --no-wait ]; then wait=""; shift; fi
  if [ -n "$DRY" ]; then dry "WINEPREFIX=$pfx wine $*"; return 0; fi
  WINEPREFIX="$pfx" WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" "$WINE_BIN/wine" "$@" >/dev/null 2>&1 || true
  if [ -n "$wait" ]; then WINEPREFIX="$pfx" WINEDEBUG=-all "$WINE_BIN/wineserver" -w; fi
}

installed() { [ -z "$REPAIR" ] && [ "$(cat "$BASE/.installed/$1" 2>/dev/null)" = "$2" ]; }
mark_installed() { mkdir -p "$BASE/.installed"; printf '%s\n' "$2" > "$BASE/.installed/$1"; }
py() { /usr/bin/python3 -I -B "$@"; }  # (-B: no __pycache__ inside the signed app)
setting() { /usr/bin/plutil -extract "$1" raw -o - "$CONFIG" 2>/dev/null || true; }
setting_list() { /usr/bin/plutil -extract "$1" json -o - "$CONFIG" 2>/dev/null | tr -d '[]"' | tr ',' ' ' || true; }

fill_template() {  # fill_template <src> <dst>: @DROPPOD_HOME@ -> this install's folder (single-quote safe), @APP_NAME@
  local text home=${BASE//\'/\'\\\'\'}
  text=$(cat "$1"; printf x); text=${text%x}
  text=${text//@APP_NAME@/$NAME}
  printf '%s' "${text//@DROPPOD_HOME@/$home}" > "$2.tmp"
  chmod +x "$2.tmp"; mv "$2.tmp" "$2"
}

unlink_documents() {  # give a prefix its own Documents folder instead of Wine's link to ~/Documents
  local d
  for d in "$1"/drive_c/users/*/Documents; do
    [ -L "$d" ] && { rm "$d"; mkdir -p "$d"; }
  done
  return 0
}

# ---------------------------------------------------------------------------------------------------------------
# what to install: the window's choices, else the last ones (config.json)
GAMES=${DROPPOD_GAMES:-$(setting_list wanted)}
GAMES=$(echo ${GAMES:-halo3})
# their mods, from the catalogue: "<mod> <wine|mac> <extras,...|-> <name>" per line
MODS=$(py "$HERE/scripts/games.py" mods $GAMES)
mods_where() {  # mods_where <runs|extra>: the IDs of the chosen mods that run that way (wine, mac) or have that extra
  printf '%s\n' "$MODS" | awk -v w="$1" 'NF && ($2 == w || index("," $3 ",", "," w ","))' | cut -d' ' -f1
}
mod_name() { printf '%s\n' "$MODS" | awk -v m="$1" '$1 == m {$1 = $2 = $3 = ""; sub(/^ +/, ""); print}'; }
uses_wine() { [ -n "$(mods_where wine)" ]; }
uses_loopback() { [ -n "$(mods_where loopback)" ]; }  # local network addresses (Project Reclaimer binds them)
uses_steam() { [ -n "$(py "$HERE/scripts/games.py" missing /dev/null $GAMES)" ]; }  # a chosen game plays from MCC's files
GAME_FILES=${DROPPOD_GAME_FILES:-$(setting gameFiles)}
GAME_FILES=${GAME_FILES:-$BASE/Game Files}
SOURCE=${DROPPOD_SOURCE:-$(setting source)}
SOURCE=${SOURCE:-steam}

step_start() {
  bold "Starting setup: $NAME $(cat "$HERE/VERSION" 2>/dev/null || echo dev)"
  note "Installs into: $BASE"
  note "Game files: $GAME_FILES ($([ "$SOURCE" = existing ] && echo "an existing MCC folder, used in place" || echo "downloaded from Steam"))"
  note "Games: $GAMES (mods: $(echo $(printf '%s\n' "$MODS" | cut -d' ' -f1)))"
  [ -n "$DRY" ] && note "Dry run: nothing is downloaded, run in Wine or changed as admin"
  [ "$(uname -s)" = Darwin ] || die "install.sh is the Mac's setup. On Windows or Linux, open the app, or run: python3 scripts/setup.py"
  [ "$(uname -m)" = arm64 ] || die "This needs an Apple Silicon Mac (M1 or newer)."
  [ "$(sw_vers -productVersion | cut -d. -f1)" -ge 14 ] || die "This needs macOS 14 Sonoma or newer."
  case "$SOURCE" in steam|existing) ;; *) die "Unknown game files source: $SOURCE" ;; esac
  if [ "$SOURCE" = existing ]; then
    [ -d "$GAME_FILES" ] || die "The MCC folder $GAME_FILES isn't there. Is its drive connected?"
  elif [ ! -d "$GAME_FILES" ] || [ -z "$(ls -A "$GAME_FILES" 2>/dev/null)" ]; then
    # a new, empty folder of our own: uninstall may offer to delete it (never a folder the player had)
    mkdir -p "$GAME_FILES" "$BASE/.installed"; printf '%s\n' "$GAME_FILES" > "$BASE/.installed/game-files-owner"
  fi
  mkdir -p "$BASE"/{tools,scripts,logs,prefixes,cache} "$CACHE" "$GAME_FILES"
  # free space: the runtime on this Mac's disk, the missing game files where they go
  local home_free files_free need
  home_free=$(df -g "$BASE" | awk 'NR==2 {print $4}')
  files_free=$(df -g "$GAME_FILES" | awk 'NR==2 {print $4}')
  need=0
  if [ -z "$UPDATE" ]; then
    MISSING=$(echo $(py "$HERE/scripts/games.py" missing "$GAME_FILES" $GAMES))  # (echo: one line, or empty)
    for d in $MISSING; do
      need=$(py "$HERE/scripts/games.py" describe "$d" | awk -v n="$need" '{printf "%.1f", n + $2}')  # (GiB, as df -g)
    done
  else
    MISSING=""
  fi
  note "Free: ${home_free} GB here, ${files_free} GB for the game files (needs ${need%.*} GB)"
  [ "$home_free" -ge "$RUNTIME_GB" ] || [ -x "$WINE_BIN/wine" ] || ! uses_wine ||
    die "Need about $RUNTIME_GB GB free on this Mac's disk (have ${home_free} GB)."
  awk -v f="$files_free" -v n="$need" 'BEGIN {exit !(f + 0.5 >= n)}' ||
    die "Need about ${need%.*} GB free for the game files (have ${files_free} GB). Free up space or choose another folder."
  # a first install in progress: if it stops, the window offers setup again (Try Again picks up where it stopped)
  [ -f "$BASE/version" ] || [ -n "$DRY" ] || touch "$BASE/.setup-incomplete"
  return 0
}

# One password prompt for everything that needs one: Rosetta (if missing, for Wine) and, for the mods with the loopback
# extra, the local network addresses they bind (Project Reclaimer: 127.0.0-3.x, 127.3.1-3.x; macOS only enables
# 127.0.0.1), as a startup task.
step_admin() {
  local cmds="" why="" lb_dir="/Library/Application Support/droppod"
  if uses_wine && ! /usr/bin/pgrep -q oahd; then
    cmds="softwareupdate --install-rosetta --agree-to-license"
    why="Rosetta (Apple's Intel translator)"
  fi
  if uses_loopback && { ! cmp -s "$HERE/scripts/droppod-loopback.sh" "$lb_dir/droppod-loopback.sh" ||
                      [ ! -f /Library/LaunchDaemons/local.droppod.loopback.plist ] || [ -n "$REPAIR" ]; }; then
    cmds="${cmds:+$cmds && }mkdir -p '$lb_dir' &&
      install -o root -g wheel -m 755 '$HERE/scripts/droppod-loopback.sh' '$lb_dir/droppod-loopback.sh' &&
      install -o root -g wheel -m 644 '$HERE/scripts/local.droppod.loopback.plist' /Library/LaunchDaemons/local.droppod.loopback.plist &&
      { launchctl bootout system/local.droppod.loopback 2>/dev/null; true; } &&
      launchctl bootstrap system /Library/LaunchDaemons/local.droppod.loopback.plist"
    why="${why:+$why and }the local network addresses $(mod_name "$(mods_where loopback | head -1)") needs"
  fi
  [ -n "$cmds" ] || return 0
  bold "Mac settings: $why (one password prompt)"
  note "The network addresses come from a small startup task: /Library/LaunchDaemons/local.droppod.loopback.plist"
  as_admin "$cmds" "$NAME is setting up $why." ||
    die "The Mac settings weren't changed (the password box was cancelled or refused). Press Try Again."
  note "ok"
}

step_wine() {
  uses_wine || return 0
  bold "Wine (Gcenx Wine Staging 11.18) with MoltenVK 1.4.2"
  if [ ! -x "$WINE_BIN/wine" ]; then
    local pkg; pkg=$(fetch "$WINE_PKG")
    if [ -n "$DRY" ]; then dry "unpack $(basename "$pkg") into $BASE/wine"
    else mkdir -p "$BASE/wine"; tar -xf "$pkg" -C "$BASE/wine"; fi
  fi
  # Wine bundles MoltenVK 1.4.0; 1.4.2 fixes device-loss / argument-buffer bugs behind a GPU address fault seen on
  # heavy maps, and used ~25% less GPU at the menu
  local mvk="$WINE_DIR/lib/libMoltenVK.dylib"
  # (grep -c reads everything: an early-exiting grep SIGPIPEs strings, and under pipefail the check always failed)
  if [ ! -f "$mvk" ] || [ "$(strings "$mvk" | grep -cx '1.4.2')" = 0 ]; then
    local pkg; pkg=$(fetch "$MOLTENVK_PKG")
    if [ -n "$DRY" ]; then dry "put MoltenVK 1.4.2's libMoltenVK.dylib into Wine"
    else
      tar -xf "$pkg" -C "$CACHE" MoltenVK/MoltenVK/dynamic/dylib/macOS/libMoltenVK.dylib
      # replace, don't overwrite in place: macOS caches a signed library's code signature per file, and Rosetta then
      # refuses the rewritten file ("Attachment of code signature supplement failed")
      rm -f "$mvk"
      cp "$CACHE/MoltenVK/MoltenVK/dynamic/dylib/macOS/libMoltenVK.dylib" "$mvk"
    fi
  fi
  note "ok"
}

step_dxvk() {
  uses_wine || return 0
  bold "Graphics translation (DXVK-macOS 1.10.3: Direct3D 11 to Vulkan)"
  if [ ! -f "$CACHE/$DXVK_DIR/x64/d3d11.dll" ]; then
    local pkg; pkg=$(fetch "$DXVK_PKG")
    if [ -n "$DRY" ]; then dry "unpack $(basename "$pkg") into $CACHE"; else tar -xzf "$pkg" -C "$CACHE"; fi
  fi
  note "ok"
}

step_depotdownloader() {
  uses_steam || return 0
  bold "DepotDownloader 3.4.0 (downloads your game files from your Steam account)"
  if [ ! -x "$BASE/tools/DepotDownloader" ] || [ -n "$REPAIR" ]; then
    local pkg; pkg=$(fetch "$DEPOT_PKG")
    if [ -n "$DRY" ]; then dry "unzip $(basename "$pkg") into $BASE/tools"
    else unzip -oq "$pkg" -d "$BASE/tools"; chmod +x "$BASE/tools/DepotDownloader"; fi
  fi
  note "ok"
}

step_scripts() {
  bold "Helper scripts"
  local f
  for f in mods.json games.py mods.py steam.py updater.py tools.py presets.py xiso.py droppod-loopback.sh \
           local.droppod.loopback.plist; do
    cp "$HERE/scripts/$f" "$BASE/scripts/$f"
  done
  cp "$HERE/APP_NAME" "$BASE/scripts/APP_NAME" 2>/dev/null || echo "$NAME" > "$BASE/scripts/APP_NAME"
  fill_template "$HERE/scripts/play.sh" "$BASE/scripts/play.sh"
  fill_template "$HERE/scripts/run_wine.sh" "$BASE/scripts/run_wine.sh"
  chmod +x "$BASE/scripts/"*.py
  note "ok"
}

# Each Wine mod gets its own prefix, set up as the Project Reclaimer Mac port does it: DXVK's d3d11.dll renamed
# d3d11_dxvk.dll behind spinfix's d3d11.dll for mods with the spinfix extra (it forwards to DXVK and stops Halo 3's
# engine thread spinning a full CPU core on the clock between frames, see spinfix/spinfix.c; plain DXVK otherwise),
# and a stand-in Steam install: the game files linked into the prefix's Steam folder, and its Workshop folder
step_prefix() {  # step_prefix <mod>
  local mod=$1 pfx="$BASE/prefixes/$1"
  bold "Windows environment for $(mod_name "$mod")"
  if [ ! -f "$pfx/system.reg" ]; then
    wine_run "$pfx" wineboot -i
  fi
  [ -n "$DRY" ] || unlink_documents "$pfx"
  local sys32="$pfx/drive_c/windows/system32" spinfix=""
  mods_where spinfix | grep -qx "$mod" && spinfix=1
  if ! installed "$mod-dxvk" "${DXVK_PKG%%|*}${spinfix:+ spinfix}" || [ ! -f "$sys32/d3d10core.dll" ]; then
    if [ -n "$DRY" ]; then dry "copy DXVK's x64 d3d10core.dll and d3d11.dll${spinfix:+ (as d3d11_dxvk.dll)} into $sys32"
    else
      cp "$CACHE/$DXVK_DIR/x64/d3d10core.dll" "$sys32/"
      cp "$CACHE/$DXVK_DIR/x64/d3d11.dll" "$sys32/d3d11${spinfix:+_dxvk}.dll"
      mark_installed "$mod-dxvk" "${DXVK_PKG%%|*}${spinfix:+ spinfix}"
    fi
  fi
  if [ -n "$spinfix" ]; then
    [ "$(shasum -a 256 "$HERE/spinfix/d3d11.dll" | cut -d' ' -f1)" = "$SPINFIX_SHA" ] || die "checksum mismatch for spinfix/d3d11.dll"
    if [ -n "$DRY" ]; then dry "copy spinfix's d3d11.dll (sha256 $SPINFIX_SHA) into $sys32"
    else cp "$HERE/spinfix/d3d11.dll" "$sys32/d3d11.dll"; fi
  fi
  # the game files, wherever they are, where the mod looks for them (C:\Program Files (x86)\Steam\...)
  local link="$pfx/$MCC_REL"
  mkdir -p "$(dirname "$link")" "$pfx/$STEAM_REL/steamapps/workshop/content/976730"
  if [ -L "$link" ] || [ ! -e "$link" ]; then ln -sfn "$GAME_FILES" "$link"; fi
  note "game files linked: $link -> $(readlink "$link" || echo "?")"
  note "ok"
}

step_mesa() {  # step_mesa <mod>
  local mod=$1 dir="$BASE/mods/$1"
  bold "Menu library (Mesa) for $(mod_name "$mod")"
  # its menu needs OpenGL 3.0+, which Wine on macOS only gives core-profile apps: Mesa's beside the exe
  mkdir -p "$dir"
  if ! installed "$mod-mesa" "${MESA_PKG%%|*}" || [ ! -f "$dir/opengl32.dll" ]; then
    local pkg; pkg=$(fetch "$MESA_PKG")
    if [ -n "$DRY" ]; then dry "unpack x64/opengl32.dll and x64/libgallium_wgl.dll from $(basename "$pkg") into $dir"
    else
      tar -xf "$pkg" -C "$CACHE" x64/opengl32.dll x64/libgallium_wgl.dll
      cp "$CACHE/x64/opengl32.dll" "$CACHE/x64/libgallium_wgl.dll" "$dir/"
      mark_installed "$mod-mesa" "${MESA_PKG%%|*}"
    fi
  fi
  note "ok"
}

wine_builtin() { LC_ALL=C grep -a -q 'Wine builtin DLL' "$1" 2>/dev/null; }
vc_installed() {
  local f
  for f in msvcp140.dll msvcp140_1.dll msvcp140_2.dll vcruntime140.dll vcruntime140_1.dll concrt140.dll; do
    [ -f "$1/$f" ] && ! wine_builtin "$1/$f" || return 1
  done
}
webview_installed() {
  local d
  for d in "$1"/[0-9]*; do
    [ -f "$d/msedgewebview2.exe" ] && [ -f "$d/msedge_elf.dll" ] && return 0
  done
  return 1
}

step_msruntimes() {  # step_msruntimes <mod>: Microsoft's DLLs rather than Wine's builtins
  local mod=$1 pfx="$BASE/prefixes/$1" pkg f
  local sys32="$pfx/drive_c/windows/system32"
  local edge="$pfx/drive_c/Program Files (x86)/Microsoft/EdgeWebView/Application"
  bold "Microsoft runtimes for $(mod_name "$mod")"
  if ! installed "$mod-vcredist" "${VC_REDIST_PKG##*|}" || ! vc_installed "$sys32"; then
    pkg=$(fetch "$VC_REDIST_PKG")
    if [ -z "$DRY" ]; then
      # The redist installer otherwise leaves Wine's existing builtin files in place.
      for f in "$sys32"/msvcp140*.dll "$sys32"/vcruntime140*.dll "$sys32/concrt140.dll"; do
        if wine_builtin "$f"; then rm "$f"; fi
      done
    fi
    # On repairs this prefix may already have EdgeUpdate running. The installer waits for its own work; do not
    # wait for unrelated background processes in the Wine session, and check the installed DLLs below instead.
    wine_run "$pfx" --no-wait "$pkg" /install /quiet /norestart
    if [ -z "$DRY" ]; then
      vc_installed "$sys32" || die "Microsoft's Visual C++ runtime did not install. Press Try Again."
      mark_installed "$mod-vcredist" "${VC_REDIST_PKG##*|}"
    fi
  fi
  if ! webview_installed "$edge"; then
    pkg=$(fetch "$WEBVIEW2_PKG")
    # The installer waits for its work, but leaves EdgeUpdate /c running in the background. Waiting for the whole
    # Wine session here would leave setup stuck after installation succeeded; check the installed files instead.
    wine_run "$pfx" --no-wait "$pkg" /silent /install
    if [ -z "$DRY" ]; then
      webview_installed "$edge" || die "Microsoft Edge WebView2 did not install. Press Try Again."
      mark_installed "$mod-webview2" "${WEBVIEW2_PKG##*|}"
    fi
  fi
  local compiler="$GAME_FILES/Engine/Binaries/ThirdParty/CEF3/Win64/d3dcompiler_47.dll"
  if [ -n "$DRY" ]; then dry "copy MCC's shader compiler $compiler into $sys32"
  else
    [ -f "$compiler" ] || die "MCC's d3dcompiler_47.dll is missing. Repair the shared MCC game files and try again."
    cmp -s "$compiler" "$sys32/d3dcompiler_47.dll" || cp "$compiler" "$sys32/d3dcompiler_47.dll"
  fi
  note "ok"
}

step_chromehoist() {
  local mod=$1
  bold "WebView2 menu fix for $(mod_name "$mod")"
  [ "$(shasum -a 256 "$HERE/chromehoist/msedge_elf.dll" | cut -d' ' -f1)" = "$CHROMEHOIST_SHA" ] ||
    die "checksum mismatch for chromehoist/msedge_elf.dll"
  if [ -n "$DRY" ]; then dry "install chromehoist (sha256 $CHROMEHOIST_SHA) into the newest WebView2 runtime"
  else
    mkdir -p "$BASE/chromehoist"
    cp "$HERE/chromehoist/msedge_elf.dll" "$BASE/chromehoist/msedge_elf.dll"
    DROPPOD_HOME="$BASE" py "$BASE/scripts/mods.py" prepare "$mod" || die "Could not install the WebView2 menu fix."
    mark_installed "$mod-chromehoist" "$CHROMEHOIST_SHA"
  fi
  note "ok"
}

step_mod() {  # step_mod <mod>: its latest release, unless it's there (it updates itself from then on)
  local mod=$1
  bold "Mod: $(mod_name "$mod") (latest release, checksum-verified)"
  # never bundled: mods.py fetches it from the mod's own GitHub releases and checks it against their checksum file
  local out; out=$(DROPPOD_HOME="$BASE" DROPPOD_DRY_RUN="$DRY" DROPPOD_REPAIR="$REPAIR" \
    py "$BASE/scripts/mods.py" get "$mod" 2>>"$BASE/logs/launcher.log" | while IFS= read -r line; do
      case "$line" in
        (progress:*|status:*) [ -n "$GUI" ] && printf '%s\n' "$line" >&3 ;;  # (the window shows these)
        (*) printf '%s\n' "$line" ;;
      esac
    done)
  printf '%s\n' "$out" | sed 's/^/    /'
  case "$(printf '%s\n' "$out" | tail -1)" in
    ok|"ok already") ;;
    "error: "*) die "$(printf '%s\n' "$out" | tail -1 | cut -c8-)" ;;
    *) die "Could not download $(mod_name "$mod"). Check your internet connection and press Try Again." ;;
  esac
}

step_presets() {  # step_presets <mod>
  local mod=$1 settings
  case "$mod" in
    reclaimer) settings='Documents/My Games/Project Reclaimer/settings.json' ;;
    renaissance) settings='AppData/Local/ReachRenaissance/settings.txt' ;;
    *) die "No graphics settings path for $mod" ;;
  esac
  # only a fresh install (or DROPPOD_RESET_GRAPHICS=1) gets the tuned settings; otherwise the player's own stay
  if [ -n "${DROPPOD_RESET_GRAPHICS:-}" ] ||
     ! ls "$BASE/prefixes/$mod"/drive_c/users/*/"$settings" >/dev/null 2>&1; then
    bold "Tuned graphics settings for $(mod_name "$mod")"
    if [ -n "$DRY" ]; then dry "presets.py $mod (recommended graphics, 60 fps)"
    else DROPPOD_HOME="$BASE" py "$BASE/scripts/presets.py" "$mod" | sed 's/^/    /'; fi
  fi
}

step_mods() {  # every chosen game's mod, with what it needs
  local mod
  for mod in $(mods_where wine); do
    step_prefix "$mod"
    mods_where mesa | grep -qx "$mod" && step_mesa "$mod"
    mods_where msruntimes | grep -qx "$mod" && step_msruntimes "$mod"
    mods_where chromehoist | grep -qx "$mod" && step_chromehoist "$mod"
    step_mod "$mod"
    mods_where presets | grep -qx "$mod" && step_presets "$mod"
  done
  for mod in $(mods_where mac); do
    step_mod "$mod"
  done
  return 0
}

step_launchers() {
  bold "Game launchers (the library's Play buttons)"
  uses_wine && note "Wine mods: $BASE/scripts/run_wine.sh <mod>"
  note "Play buttons: $BASE/scripts/play.sh <game> [mod] (default: config.json playWith, else the game's first mod)"
  note "ok"
}

# Game apps: one per Wine mod (<home>/game.noindex/<mod's name>.app), so a running mod shows as itself (its name in the
# menu bar, the Dock, Cmd-Tab and Game Mode) instead of "wine". Its executable is a launch script (made from
# scripts/game-launch.sh.in) that hands over to play.sh with DROPPOD_WINE set to a COPY of Wine's loader beside it
# (a copy: the game process is that file; Wine finds its libraries relative to it, through Contents/lib and
# Contents/share, links into the Wine install). Processes the mod starts itself (Project Reclaimer's after its update
# check) still show as "wine": Wine starts them with its own loader. No icon of its own: Wine shows the program's.
# Kept in a .noindex folder, so Spotlight doesn't list them. Not inside the app itself: the loader only exists after
# Wine is installed, and writing into the signed app would break its signature.
step_game_app() {
  uses_wine || return 0  # (the Mac apps among the mods show as themselves)
  bold "Game app (each mod shows as itself in the menu bar and Dock)"
  rm -rf "$BASE/game.noindex/$NAME.app"  # (one for all mods, named after this app, before 0.1.1)
  local mod name app xname
  for mod in $(mods_where wine); do
    name=$(mod_name "$mod")
    app="$BASE/game.noindex/$name.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    if [ -n "$DRY" ]; then dry "copy Wine's loader $WINE_BIN/wine to $app/Contents/MacOS/$name"
    else
      # replace, don't overwrite in place (macOS caches a binary's signature per file)
      cp "$WINE_BIN/wine" "$app/Contents/MacOS/$name.tmp"; mv "$app/Contents/MacOS/$name.tmp" "$app/Contents/MacOS/$name"
    fi
    ln -sfn "$WINE_DIR/lib" "$app/Contents/lib"
    ln -sfn "$WINE_DIR/share" "$app/Contents/share"
    fill_template "$HERE/scripts/game-launch.sh.in" "$app/Contents/MacOS/launch"
    rm -f "$app/Contents/Resources/AppIcon.icns"
    xname=$(printf '%s' "$name" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g')
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$xname</string>
  <key>CFBundleDisplayName</key><string>$xname</string>
  <key>CFBundleIdentifier</key><string>local.droppod.game.$mod</string>
  <key>CFBundleExecutable</key><string>launch</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$(cat "$HERE/VERSION" 2>/dev/null || echo dev)</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
</dict>
</plist>
PLIST
    if [ -n "$DRY" ]; then dry "codesign --force --sign - $app"
    else codesign --force --sign - "$app" 2>/dev/null || true; fi
    note "$app"
  done
  note "ok"
}

step_game_files() {
  uses_steam || return 0
  bold "Game files ($([ "$SOURCE" = existing ] && echo "your MCC folder" || echo "from your Steam account"))"
  local present; present=$(echo $(py "$HERE/scripts/games.py" present "$GAME_FILES"))
  note "already there: ${present:-nothing}"
  if [ -z "$MISSING" ]; then note "nothing to download"; return 0; fi
  local d
  for d in $MISSING; do
    note "to download: $(py "$HERE/scripts/games.py" describe "$d")"
  done
  if [ -n "$UPDATE" ]; then note "skipped while updating"; return 0; fi
  if [ -n "$GUI" ] || [ -n "$DRY" ]; then
    # the window shows the QR code and a progress bar; the account name is saved, so Get and Repair don't ask again
    DROPPOD_HOME="$BASE" DROPPOD_DRY_RUN="$DRY" py "$BASE/scripts/steam.py" download "$GAME_FILES" $MISSING \
      2>>"$BASE/logs/launcher.log" || exit 1  # (it printed the reason)
  else
    note "You need to own Halo: The Master Chief Collection on Steam."
    note "A QR code will appear: open the Steam app on your phone > Steam Guard > Scan a QR code."
    read -r -p "    Press Return to continue..." _ </dev/tty
    # shellcheck disable=SC2086
    "$BASE/tools/DepotDownloader" -app 976730 -depot $MISSING -os windows -qr -remember-password -dir "$GAME_FILES" -validate </dev/tty ||
      die "The Steam download stopped. Run the installer again to resume."
  fi
  if [ -z "$DRY" ]; then
    local left; left=$(echo $(py "$HERE/scripts/games.py" missing "$GAME_FILES" $GAMES))
    [ -z "$left" ] || die "Some game files are still missing after the download ($left). Press Try Again to resume."
  fi
  note "ok"
}

step_config() {
  bold "Settings file (config.json)"
  DROPPOD_HOME="$BASE" py "$HERE/scripts/games.py" write-config "$CONFIG" "$GAME_FILES" "$SOURCE" $GAMES | sed 's/^/    /'
  note "$CONFIG"
}

step_app() {
  local src=${DROPPOD_APP_SOURCE:-}
  [ -n "$src" ] && [ -d "$src" ] || return 0
  case "$src" in /Applications/*|"$APPS"/*) return 0 ;; esac  # (already installed: it is the one that runs)
  bold "Adding $NAME to Applications"
  local dest="$APPS/$NAME.app"
  [ -d "/Applications/$NAME.app" ] && dest="/Applications/$NAME.app"
  if [ -n "$DRY" ]; then dry "copy $src to $dest"; return 0; fi
  mkdir -p "$(dirname "$dest")"
  # replace, don't overwrite in place (macOS caches a binary's signature per file); copies from the disk image carry
  # its quarantine flag, which would ask for Open Anyway again
  rm -rf "$dest.new"; /usr/bin/ditto "$src" "$dest.new"
  /usr/bin/xattr -dr com.apple.quarantine "$dest.new" 2>/dev/null || true
  rm -rf "$dest"; mv "$dest.new" "$dest"
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$dest" || true
  note "$dest"
}

step_finish() {
  [ -n "$DRY" ] && { bold "Done (dry run)"; return 0; }
  cp "$HERE/VERSION" "$BASE/version" 2>/dev/null || echo dev > "$BASE/version"
  rm -f "$BASE/.setup-incomplete"
  rm -rf "$CACHE/x64" "$CACHE/MoltenVK"
  # the downloaded archives (~1 GB) aren't needed any more, unless they're a cache the caller asked to keep
  # (DXVK's unpacked folder stays: Repair copies it into the prefixes again)
  [ -z "${DROPPOD_CACHE:-}" ] && find "$CACHE" -maxdepth 1 -type f -delete
  [ -n "$UPDATE" ] && { bold "Updated to $(cat "$BASE/version")"; return 0; }
  bold "Done"
  note "Open $NAME from Launchpad, Spotlight or your Applications folder and press Play on a game."
}

# ---------------------------------------------------------------------------------------------------------------
# In order. Each step skips what's already done, so Try Again (and a tile's Get) picks up where it stopped.
MISSING=""
# anything failing without its own message still tells the window which step it was (the window shows the first
# error line, so a specific one from die or steam.py wins)
[ -n "$GUI" ] && trap 'rc=$?; [ $rc = 0 ] || printf "error: Something went wrong at the step “%s”. Check your internet connection and disk space, then press Try Again.\n" "$STEP" >&3' EXIT
step_start
step_admin
step_wine
step_dxvk
step_depotdownloader
step_scripts
# Reach's runtime setup copies the shader compiler from the shared game files.
step_game_files
step_mods
step_launchers
step_game_app
step_config
step_app
step_finish
