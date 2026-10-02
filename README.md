# Legacy of Goku X

Reverse engineering of the three Webfoot DBZ GBA action RPGs (The Legacy of
Goku, The Legacy of Goku II, Buu's Fury), a local dashboard of everything
learned, and a browser battle simulator built on the real mechanics.

ROMs are not in the repo. Put them in `roms/` as `log1.gba`, `log2.gba`, `buu.gba`.

## Setup

    python3 -m venv .venv && .venv/bin/pip install capstone numpy pillow cmake
    sh tools/emu/build.sh      # builds mGBA + the headless emulator shim

`tools/emu/emu.py` is a scriptable emulator (frames, input, memory, screenshots,
save states, watchpoints/breakpoints).
