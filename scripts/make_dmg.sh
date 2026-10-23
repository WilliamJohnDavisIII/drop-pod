#!/bin/bash
# make_dmg.sh [out.dmg]: build dist/<Name>.dmg (spaces as dashes, e.g. Drop-Pod.dmg): the app (it sets everything up
# on first open, then copies itself into Applications), a picture of macOS's one-time "Open Anyway" step, a read-me
# and the credits, with the app's icon as the volume's icon. Also writes dist/<Name>.zip and dist/SHA256SUMS, what a GitHub release needs for self-update.
# Runs scripts/make_app.sh first. Adapted from the Project Reclaimer Mac port's make_dmg.sh (MIT).
set -euo pipefail
cd "$(dirname "$0")/.."
NAME=$(cat APP_NAME)
FILE=${NAME// /-}
OUT=${1:-dist/$FILE.dmg}
bash scripts/make_app.sh
APP="dist/$NAME.app"
STAGE=$(mktemp -d)/"$NAME"
mkdir -p "$STAGE" "$(dirname "$OUT")"
/usr/bin/ditto "$APP" "$STAGE/$NAME.app"
ln -s /Applications "$STAGE/Applications"

"$APP/Contents/MacOS/DropPod" --render-guide "$STAGE" >/dev/null
mv "$STAGE/guide.png" "$STAGE/How to open it the first time.png"
cp CREDITS.md "$STAGE/Credits and licences.md"

sed "s/@NAME@/$NAME/g" > "$STAGE/Read Me First.txt" <<'TXT'
@NAME@ sets up community-made ways to play the Halo campaigns on your Mac, with split-screen co-op:
Project Reclaimer for Halo 3, from your own Steam copy of Halo: The Master Chief Collection, and the
native port of Halo CE, from your own Xbox disc.

You need: an Apple Silicon Mac (M1 or newer) on macOS 14 or newer, Halo: The Master Chief Collection
on Steam, the Steam app on your phone, and your Mac password once. Disk space: about 3 GB on your Mac,
plus 33 GB of Halo 3's game files (an external drive works).

1. Drag "@NAME@" onto "Applications" (or just double-click it here).
2. Double-click "@NAME@". The first time, macOS blocks it because it isn't from the App Store.
   The picture "How to open it the first time" shows how to allow it (System Settings >
   Privacy & Security > Open Anyway). You only do this once.
3. The @NAME@ window checks your Mac, then asks where the game files are:
   - "Download them from Steam": when it shows a QR code, scan it with the Steam app on your phone
     (Steam Guard > Scan a QR code). The games download from your own Steam account.
   - "Use a game files folder I already have": point it at the Halo game files you already have
     (for example copied from a PC). They're used where they are, so there's nowhere else to choose:
     any game you picked that the folder doesn't have yet is downloaded into it.
   macOS asks for your Mac password once.
4. When it's done, it offers native Halo CE: the original Xbox Halo rebuilt for the Mac. It needs a
   disc image (.iso) of your own Halo: Combat Evolved Xbox disc, and guides you through it. You can
   also do it later from the Halo CE tile.
5. Then you see your games. Press Play.

Settings (top right) has the game files location, Repair, the logs and Uninstall.
You can eject this disk image and move it to the Trash once @NAME@ is set up.
TXT

# the volume's icon: .VolumeIcon.icns plus the volume's custom-icon flag, set on a writable image, then compressed
rm -f "$OUT"
RW=$(dirname "$STAGE")/rw.dmg
MNT=$(dirname "$STAGE")/mnt
hdiutil create -quiet -volname "$NAME" -srcfolder "$STAGE" -fs HFS+ -format UDRW "$RW"
mkdir "$MNT"
hdiutil attach -quiet -nobrowse -noautoopen -mountpoint "$MNT" "$RW"
cp "$APP/Contents/Resources/AppIcon.icns" "$MNT/.VolumeIcon.icns"
SetFile -a C "$MNT"
hdiutil detach -quiet "$MNT"
hdiutil convert -quiet "$RW" -format UDZO -o "$OUT"
rm -rf "$(dirname "$STAGE")"

# the release files the app's updater looks for (updater.py: <Name>.zip + SHA256SUMS)
rm -f "dist/$FILE.zip"
/usr/bin/ditto -c -k --keepParent "$APP" "dist/$FILE.zip"
(cd dist && shasum -a 256 "$FILE.zip" "$(basename "$OUT")" > SHA256SUMS)
echo "$OUT"
cat dist/SHA256SUMS
