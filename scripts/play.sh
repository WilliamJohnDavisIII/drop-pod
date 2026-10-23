#!/bin/zsh
# play.sh <game> [mod]: what a library tile's Play button runs. Output goes to logs/<game>.log (emptied first).
# The library runs Wine mods through their game app (game.noindex/<mod's name>.app, see game-launch.sh.in), so the
# game shows as the mod, not "wine". The mod is the tile's "Play with" choice (config.json playWith.<game>), else the game's
# first in the catalogue (mods.json). How it starts comes from there too:
#   wine   run_wine.sh <mod>: the mod's program under Wine, in its own prefix (Project Reclaimer: Halo 3)
#   mac    the mod's own Mac app, from mods/<mod>/ (ChupathingyCE: Halo CE, from the player's own Xbox disc)
BASE=${DROPPOD_HOME:-'@DROPPOD_HOME@'}
CONFIG="$BASE/config.json"
GAME=${1:?usage: play.sh <game> [mod]}
CHOICE=${2:-$(/usr/bin/plutil -extract "playWith.$GAME" raw -o - "$CONFIG" 2>/dev/null)}
LOG="$BASE/logs/$GAME.log"
mods() { /usr/bin/python3 -I -B "$BASE/scripts/mods.py" "$@"; }
mkdir -p "$BASE/logs"
: > "$LOG"
read -r MOD RUNS <<< "$(mods which "$GAME" $CHOICE 2>> "$LOG")"
case "$RUNS" in
  wine) exec "$BASE/scripts/run_wine.sh" "$MOD" > "$LOG" 2>&1 ;;
  mac)
    APP=$(mods program "$MOD")
    [ -d "$APP" ] || { echo "$MOD isn't set up yet" >> "$LOG"; exit 1; }
    # -W: wait until it quits, so the window knows the game ended; -n: a fresh instance
    exec /usr/bin/open -W -n "$APP" >> "$LOG" 2>&1 ;;
  *) echo "play.sh: $GAME can't play with ${CHOICE:-its mod}" >&2; exit 2 ;;
esac
