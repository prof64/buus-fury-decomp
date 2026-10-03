#!/bin/sh
# The emulator shim with cycle timestamps on breakpoint / watchpoint hits (menu redraw timing). Needs
# tools/emu/build.sh's libmgba (tools/mgba/build/libmgba.a). Output: re/buu/menu/emut/libemut.dylib
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
M="$HERE/../../../../tools/mgba"
DEFS=$(grep -m1 "DEFINES = " "$M/build/build.ninja" | sed 's/.*DEFINES = //')
clang -O2 -shared -fPIC -o "$HERE/libemut.dylib" "$HERE/shim.c" \
  -I"$M/include" -I"$M/src" -I"$M/build/include" $DEFS \
  "$M/build/libmgba.a" -L/opt/homebrew/lib -lz -lpng -lfreetype -llzma -framework CoreFoundation
echo built "$HERE/libemut.dylib"
