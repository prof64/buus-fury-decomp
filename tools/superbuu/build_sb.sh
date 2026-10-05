#!/bin/sh
set -e
cd "$(dirname "$0")/../.."
mkdir -p work/superbuu
grep -m1 "DEFINES = " tools/mgba/build/build.ninja | sed 's/.*DEFINES = //' > work/superbuu/defs.txt
M=tools/mgba
DEFS=$(cat work/superbuu/defs.txt)
# the current shim + the savedata accessors (one translation unit)
cat tools/emu/shim.c tools/superbuu/shim_sb.c > work/superbuu/shim_sb_full.c
# (built beside, then renamed: processes that have the old library loaded keep their copy)
clang -O2 -shared -fPIC -o work/superbuu/libemu_sb.new.dylib work/superbuu/shim_sb_full.c -I$M/include -I$M/src -I$M/build/include $DEFS $M/build/libmgba.a -L/opt/homebrew/lib -lz -lpng -lfreetype -llzma -framework CoreFoundation
mv work/superbuu/libemu_sb.new.dylib work/superbuu/libemu_sb.dylib
echo ok
