#!/bin/bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Devon Fuller
# Rebuild chromehoist/msedge_elf.dll (the WebView2 menu fix for Reach Renaissance under Wine on the Mac) with Zig's
# clang: `zig cc -target x86_64-windows-gnu` (Zig 0.13 or newer, https://ziglang.org/download; ZIG=/path/to/zig if
# it isn't on PATH). mingw-w64 should work too (x86_64-w64-mingw32-gcc ... -luser32 -lkernel32), untested.
# Then put the new SHA-256 in install.sh (CHROMEHOIST_SHA).
#   build.sh [path/to/msedge_elf.dll]   with Edge's own msedge_elf.dll (EdgeWebView/Application/<version>/, the
#                                       runtime install.sh pins) first regenerates msedge_elf.def from its exports
set -euo pipefail
cd "$(dirname "$0")"
if [ -n "${1:-}" ]; then python3 -I exports.py "$1" > msedge_elf.def; echo "msedge_elf.def: $(grep -c '=' msedge_elf.def) exports"; fi
"${ZIG:-zig}" cc -target x86_64-windows-gnu -O2 -shared -s -Wall -o msedge_elf.dll chromehoist.c msedge_elf.def -luser32 -lkernel32
rm -f chromehoist.lib  # (the import library zig writes beside the DLL: not needed)
shasum -a 256 msedge_elf.dll
