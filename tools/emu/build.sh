#!/bin/sh
# Builds libmgba (if needed) and the ctypes shim. Run from anywhere.
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$HERE/../.."
M="$HERE/../mgba"
[ -d "$M" ] || git clone -q --depth 1 https://github.com/mgba-emu/mgba.git "$M"
if [ ! -f "$M/build/libmgba.a" ]; then
  "$ROOT/.venv/bin/cmake" -S "$M" -B "$M/build" -G Ninja -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_QT=OFF -DBUILD_SDL=OFF -DBUILD_GL=OFF -DBUILD_GLES2=OFF -DBUILD_GLES3=OFF \
    -DUSE_FFMPEG=OFF -DUSE_LUA=OFF -DUSE_EDITLINE=OFF -DUSE_ELF=OFF -DUSE_SQLITE3=OFF \
    -DUSE_LIBZIP=OFF -DUSE_MINIZIP=OFF -DUSE_DISCORD_RPC=OFF -DENABLE_SCRIPTING=OFF \
    -DM_CORE_GB=OFF -DBUILD_STATIC=ON -DBUILD_SHARED=OFF -DUSE_DEBUGGERS=ON
  ninja -C "$M/build"
fi
DEFS=$(grep -m1 "DEFINES = " "$M/build/build.ninja" | sed 's/.*DEFINES = //')
clang -O2 -shared -fPIC -o "$HERE/libemu.dylib" "$HERE/shim.c" \
  -I"$M/include" -I"$M/src" -I"$M/build/include" $DEFS \
  "$M/build/libmgba.a" -L/opt/homebrew/lib -lz -lpng -lfreetype -llzma -framework CoreFoundation
echo built "$HERE/libemu.dylib"
