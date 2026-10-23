#!/bin/bash
# make_icon.sh: draw the icon (assets/icon/make_icon.py), render it with WebKit (assets/icon/tools/render.swift), each
# size from the SVG at its own size, and build assets/icon/Droppod.icns and droppod-icon-1024.png from it.
set -euo pipefail
cd "$(dirname "$0")/../assets/icon"
T=$(mktemp -d)
/usr/bin/python3 -I -B make_icon.py
swiftc -O tools/render.swift -o "$T/render"
mkdir "$T/Droppod.iconset"
for s in 16 32 128 256 512; do
  "$T/render" droppod-icon.svg "$T/Droppod.iconset/icon_${s}x${s}.png" $s
  "$T/render" droppod-icon.svg "$T/Droppod.iconset/icon_${s}x${s}@2x.png" $((s * 2))
done
cp "$T/Droppod.iconset/icon_512x512@2x.png" droppod-icon-1024.png
iconutil -c icns "$T/Droppod.iconset" -o Droppod.icns
rm -rf "$T"
echo assets/icon/Droppod.icns
