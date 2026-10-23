#!/bin/zsh
# run_wine.sh <mod>: start a mod that runs under Wine Staging + DXVK-macOS (D3D11 -> Vulkan -> MoltenVK -> Metal), in
# its own prefix, from its folder (mods/<mod>/). What it runs, with which arguments, DLL overrides and environment
# comes from the catalogue (mods.json, through mods.py launch). Adapted from the Project Reclaimer Mac port's run.sh
# (MIT); install.sh fills in @DROPPOD_HOME@.
# Each prefix keeps its own Documents folder (not ~/Documents). The game files are wherever config.json says:
# install.sh links them into the prefix's Steam folder, where the mods look.
BASE=${DROPPOD_HOME:-'@DROPPOD_HOME@'}
CONFIG="$BASE/config.json"
MOD=${1:?usage: run_wine.sh <mod>}
setting() { /usr/bin/plutil -extract "$1" raw -o - "$CONFIG" 2>/dev/null; }
export WINEDEBUG=${WINEDEBUG:--all,err+d3d,err+vulkan,err+mmdevapi,err+coreaudio}
# (the mods update themselves when they start, restarting under the same wineserver: the library waits for it)

LAUNCH=$(/usr/bin/python3 -I -B "$BASE/scripts/mods.py" launch "$MOD") || exit 1
eval "$LAUNCH"  # WINEPREFIX, MOD_DIR, EXE, ARGS, MOD_OVERRIDES and the mod's own environment
CONF_WINE=$(setting wine)
# DROPPOD_WINE: the copy of Wine's loader inside the mod's game app, so the Mac shows it as the mod (see
# game-launch.sh.in). The processes the mod starts itself still show as "wine": Wine starts them with its own loader
# and ignores WINELOADER (tried 2026-10-22).
WINE=${DROPPOD_WINE:-$CONF_WINE}
export WINESERVER=${WINESERVER:-${CONF_WINE:h}/wineserver}
[ -x "$WINE" ] && [ -d "$WINEPREFIX" ] || {
  echo "run_wine.sh: @APP_NAME@ isn't fully set up ($CONFIG). Open @APP_NAME@ and use Settings > Repair." >&2; exit 1; }
# DXVK-macOS d3d11/d3d10core in every prefix's system32 (Wine's own D3D11 cannot create a device on MoltenVK).
# For Project Reclaimer, d3d11.dll there is spinfix, which forwards to DXVK (d3d11_dxvk.dll) and makes Halo 3's engine
# thread sleep instead of spinning on the clock between frames: about one CPU core less. RECLAIMER_SPINFIX_US=0 turns
# it off.
export WINEDLLOVERRIDES="${MOD_OVERRIDES:+$MOD_OVERRIDES;}d3d11,d3d10core=n,b"
export DXVK_ASYNC=1
# DXVK logs every swapchain rebuild at info level, which bloats the log
export DXVK_LOG_LEVEL=${DXVK_LOG_LEVEL:-warn}
export MVK_CONFIG_LOG_LEVEL=1
# Controllers: SDL on macOS only gets controller input for the frontmost app unless this is set
export SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS=${SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS:-1}
[ -n "$RECLAIMER_NO_AUDIO" ] && WINEDLLOVERRIDES="$WINEDLLOVERRIDES;winecoreaudio.drv=d"  # troubleshooting only

cd "$BASE/mods/$MOD${MOD_DIR:+/$MOD_DIR}" || exit 1  # (MOD_DIR: the folder its zip unpacks to, Reach Renaissance's)
# DXVK's shader cache beside the mod, not in the game's working folder (Project Gravemind moves into MCC's)
export DXVK_STATE_CACHE_PATH=${DXVK_STATE_CACHE_PATH:-$PWD}
[ -n "$EXE" ] || { echo "$MOD is not installed in $BASE/mods/$MOD" >&2; exit 1; }
exec "$WINE" "$EXE" "${ARGS[@]}"
