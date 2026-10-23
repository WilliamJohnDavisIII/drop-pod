#!/bin/bash
# make_app.sh [test|states <dir>]: build "dist/<name>.app" (arm64, macOS 14+) from launcher/ (Rust: egui on wgpu),
# with install.sh, the scripts it installs, spinfix's d3d11.dll, the licences and the icon inside, ad-hoc signed.
#   test           run the tests instead: launcher's (cargo test) and tests/test_scripts.py
#   states <dir>   also write a PNG of every window state into <dir> (the app's --render-states)
# Needs Rust (rustup: launcher/rust-toolchain.toml picks the version). CARGO_TARGET_DIR moves the build folder.
# Adapted from the Project Reclaimer Mac port's make_launcher.sh and make_app.sh (MIT).
set -euo pipefail
cd "$(dirname "$0")/.."
export MACOSX_DEPLOYMENT_TARGET=14.0
CARGO=(cargo --quiet)
command -v cargo >/dev/null || CARGO=("$HOME/.cargo/bin/cargo" --quiet)
if [ "${1:-}" = test ]; then
  # (from inside launcher/, so rustup finds its rust-toolchain.toml)
  out=$(cd launcher && "${CARGO[@]}" test --release 2>&1) || { echo "$out" | tail -40; exit 1; }
  grep "test result" <<< "$out"
  /usr/bin/python3 -I -B tests/test_scripts.py 2>&1 | tail -3
  exit "${PIPESTATUS[0]}"
fi

VERSION=$(cat VERSION)
NAME=$(cat APP_NAME)  # the app's name: this file is the one place to change it
xml() { sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' <<< "$1"; }
APP="dist/$NAME.app"
RES="$APP/Contents/Resources"
(cd launcher && "${CARGO[@]}" build --release --target aarch64-apple-darwin)
TARGET=${CARGO_TARGET_DIR:-launcher/target}
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$RES/install/scripts" "$RES/install/spinfix"
cp "$TARGET/aarch64-apple-darwin/release/droppod" "$APP/Contents/MacOS/DropPod"

cp assets/icon/Droppod.icns "$RES/AppIcon.icns"  # (install.sh gives the game app the same icon)
cp LICENSE LICENSE.reclaimer-mac-port CREDITS.md THIRD_PARTY_NOTICES "$RES/"
# install.sh and everything it installs: the app runs it for setup, Get, Repair and after updating itself
cp install.sh VERSION APP_NAME "$RES/install/"
for f in mods.json games.py mods.py steam.py updater.py tools.py presets.py xiso.py play.sh run_wine.sh \
         game-launch.sh.in droppod-loopback.sh local.droppod.loopback.plist; do
  cp "scripts/$f" "$RES/install/scripts/$f"
done
cp spinfix/d3d11.dll "$RES/install/spinfix/d3d11.dll"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$(xml "$NAME")</string>
  <key>CFBundleDisplayName</key><string>$(xml "$NAME")</string>
  <key>CFBundleIdentifier</key><string>local.droppod.app</string>
  <key>CFBundleExecutable</key><string>DropPod</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSArchitecturePriority</key><array><string>arm64</string></array>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
  <key>NSHumanReadableCopyright</key><string>GPL-3.0-or-later (portions MIT, see LICENSE.reclaimer-mac-port). Halo is a trademark of Microsoft; this app is not affiliated with Microsoft, 343 Industries or Valve.</string>
  <key>DropPodRepo</key><string>${DROPPOD_REPO:-WilliamJohnDavisIII/drop-pod}</string>
</dict>
</plist>
PLIST
# ad-hoc signature over the whole bundle, so macOS treats the binary, Info.plist, icon and scripts as one app
codesign --force --sign - "$APP"
codesign --verify --strict "$APP"
echo "$APP ($VERSION)"

if [ "${1:-}" = states ]; then
  "$APP/Contents/MacOS/DropPod" --render-states "${2:?states <dir>}" launcher/tests/qr-sample.txt
fi
