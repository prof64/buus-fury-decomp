#!/bin/sh
set -e
cd "$(dirname "$0")/../.."
mkdir -p work/superbuu
grep -m1 "DEFINES = " tools/mgba/build/build.ninja | sed 's/.*DEFINES = //' > work/superbuu/defs.txt
M=tools/mgba
DEFS=$(cat work/superbuu/defs.txt)
clang -O2 -shared -fPIC -o work/superbuu/libemu_sb.dylib tools/superbuu/shim_sb.c -I$M/include -I$M/src -I$M/build/include $DEFS $M/build/libmgba.a -L/opt/homebrew/lib -lz -lpng -lfreetype -llzma -framework CoreFoundation
echo ok
