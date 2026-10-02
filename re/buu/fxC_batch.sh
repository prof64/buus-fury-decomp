#!/bin/sh
# enemy projectile visual traces (fxC_trace.py) for the common classes (second pass: pinned every frame)
cd "$(dirname "$0")"
PY=../../.venv/bin/python
$PY fxC_trace.py 0x01 900 40 -8 300 0 110,0 > /dev/null 2>&1
$PY fxC_trace.py 0x08 900 80 0 300 0 110,0 > /dev/null 2>&1
$PY fxC_trace.py 0x4e 900 70 0 300 0 110,0 > /dev/null 2>&1
$PY fxC_trace.py 0x3a 900 60 -8 300 0 110,0 > /dev/null 2>&1
$PY fxC_trace.py 0x05 900 40 0 300 0 110,0 > /dev/null 2>&1
$PY fxC_trace.py 0x3c 900 60 0 300 0 110,0 > /dev/null 2>&1
$PY fxC_trace.py 0x12 1200 60 0 300,600 0 110,0 > /dev/null 2>&1
echo done
