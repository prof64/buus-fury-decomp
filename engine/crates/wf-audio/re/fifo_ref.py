"""Research/validation only: capture the exact 8-bit FIFO streams (left/right) the game's sound driver writes for a
song or SFX, through tools/audio/buu_sound.Harness (game frozen, driver running). Output:
work/audio/fifo_ref/{song,sfx}_NN.bin (interleaved L,R s8 at 15978 Hz) + .json (driver state at start).

    .venv/bin/python engine/crates/wf-audio/re/fifo_ref.py song 0 [seconds]
    .venv/bin/python engine/crates/wf-audio/re/fifo_ref.py sfx 3 [seconds]
    .venv/bin/python engine/crates/wf-audio/re/fifo_ref.py all [seconds]
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'tools' / 'audio'))
from buu_sound import Harness, SOUND, PLAYER, VOICES, PLAY_SONG, PLAY_SFX, SONG_TABLE, SFX_TABLE  # noqa: E402

OUT = ROOT / 'work' / 'audio' / 'fifo_ref'
OUT.mkdir(parents=True, exist_ok=True)


def state(e):
    tracks = []
    for t in range(15):
        b = PLAYER + 0x18 + 0x60 * t
        tracks.append({'master': e.r8(b + 0x34), 'pan': e.r8(b + 0x13), 'pan49': e.r8(b + 0x49)})
    voices = [{'master': e.r8(VOICES + 0x40 * v + 0x34), 'pan': e.r8(VOICES + 0x40 * v + 0x13)} for v in range(16)]
    # raw track RAM: the driver never clears effect memories between songs, so a song's output depends on what
    # played before; the test seeds the Rust driver with this
    ram = [e.read(PLAYER + 0x18 + 0x60 * t, 0x60).hex() for t in range(15)]
    return {'tracks': tracks, 'voices': voices, 'countdown': e.r16(PLAYER + 8), 'track_ram': ram}


def capture(h, kind, i, seconds):
    e = h.e
    h.silence()
    e.w16(PLAYER + 8, 0)   # tick countdown 0: the first tick happens at the first mixed sample after the call
    st = state(e)
    start = e.r32(SOUND + 8) & 0x3FF
    if kind == 'song':
        h.call(PLAY_SONG, PLAYER, SONG_TABLE + 20 * i)
    else:
        h.call(PLAY_SFX, VOICES, SFX_TABLE + 12 * i, 0x40, 0)
    # the call ran inside this frame's state update; the driver mixed right after it.
    out = bytearray()
    pos = start
    frames = int(seconds * 59.7275) + 1
    first = True
    for f in range(frames + 1):
        if not first:
            e.run(1)
        first = False
        new = e.r32(SOUND + 8) & 0x3FF
        n = (new - pos) & 0x3FF
        if n:
            left = e.read(0x02000000, 0x400)
            right = e.read(0x02000400, 0x400)
            for k in range(n):
                p = (pos + k) & 0x3FF
                out.append(left[p])
                out.append(right[(p + 0x200) & 0x3FF])
        pos = new
    (OUT / f'{kind}_{i:02d}.bin').write_bytes(bytes(out))
    (OUT / f'{kind}_{i:02d}.json').write_text(json.dumps(st))
    h.silence()
    print(kind, i, len(out) // 2, 'samples', flush=True)


def main():
    h = Harness()
    h.e.set_volume(0)
    kind = sys.argv[1]
    if kind == 'full':
        # intro + one full loop per song (lengths from `cargo run --example song_lengths`)
        lengths = json.loads((ROOT / 'work' / 'audio' / 'song_lengths.json').read_text())
        for i in range(53):
            capture(h, 'song', i, max(60.0, lengths[str(i)]))
        for i in range(64):
            capture(h, 'sfx', i, 4)
        return
    if kind == 'all':
        secs = float(sys.argv[2]) if len(sys.argv) > 2 else 20
        for i in range(53):
            capture(h, 'song', i, secs)
        for i in range(64):
            capture(h, 'sfx', i, 4)
        return
    i = int(sys.argv[2])
    secs = float(sys.argv[3]) if len(sys.argv) > 3 else 20
    capture(h, kind, i, secs)


if __name__ == '__main__':
    main()
