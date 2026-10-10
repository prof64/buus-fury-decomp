"""Research/validation only: capture the exact 8-bit FIFO stream across sound TRANSITIONS - a song replaced by another
mid-play, SFX over music, stop all SFX, the tracks cleared, a song after a song (the driver's state carried over) -
through tools/audio/buu_sound.Harness (game frozen, driver running; the calls run in a frame's state update, the
driver mixes after them). Output work/audio/fifo_ref/trans_NAME.bin (interleaved L,R s8) + .json: the driver state
at the start (as fifo_ref.py), `chunks` (samples mixed in each frame) and `ops` [[chunk index, op, args...]] - an op
runs before that chunk's mix. Checked by tests/fifo_exact.rs `transitions_match_fifo_capture`.

    .venv/bin/python engine/crates/wf-audio/re/fifo_trans.py [NAME ...]
"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from fifo_ref import ROOT, OUT, state  # noqa: E402
from buu_sound import Harness, SOUND, PLAYER, VOICES, PLAY_SONG, PLAY_SFX, SONG_TABLE, SFX_TABLE, CLEAR_TRACKS, STOP_ALL_SFX  # noqa: E402

# (frame, op, args): 'song' i | 'sfx' i vol pan | 'stop_sfx' | 'clear' (the tracks silenced, CLEAR_TRACKS) | 'stop'
# (stop_music: the playing byte cleared before the frame)
SEQS = {
    'song_to_song': [(0, 'song', 3), (600, 'song', 7)],
    'sfx_over_song': [(0, 'song', 3), (120, 'sfx', 41, 0x40, 0), (124, 'sfx', 44, 0x30, -40), (300, 'stop_sfx'),
                      (420, 'clear'), (480, 'song', 12)],
    'vibrato_carry': [(0, 'song', 16), (240, 'song', 10), (480, 'song', 16)],
    # the scripts' stop_music (native 0x24, 0x08000900: the player's playing byte +5 = 0, nothing else), then a song
    'song_stop': [(0, 'song', 3), (300, 'stop'), (500, 'song', 7), (700, 'stop'), (701, 'song', 7)],
    # the Options' volumes mid-song (0x0803F18C music: every track's master byte; 0x0803E42A the SFX voices'): up,
    # down under a running SFX, to 0 and back
    'volume_change': [(0, 'song', 3), (200, 'volm', 8), (380, 'sfx', 41, 0x40, 0), (400, 'volm', 1), (402, 'vols', 2),
                      (420, 'sfx', 44, 0x40, 20), (600, 'volm', 0), (640, 'vols', 0), (660, 'sfx', 41, 0x40, 0),
                      (700, 'volm', 4), (702, 'vols', 6), (720, 'sfx', 41, 0x40, 0), (760, 'song', 12)],
    'quick_switch': [(0, 'song', 1), (2, 'song', 2), (5, 'song', 1), (200, 'sfx', 3, 0x40, 0), (201, 'song', 20)],
}
FRAMES = 900


def run(h, name, seq):
    e = h.e
    h.silence()
    e.w16(PLAYER + 8, 0)
    st = state(e)
    pos = e.r32(SOUND + 8) & 0x3FF
    out = bytearray()
    chunks, ops = [], []
    by = {}
    for f, *op in seq:
        by.setdefault(f, []).append(op)
    for f in range(FRAMES):
        todo = by.get(f, [])
        if todo:
            for op in todo:
                ops.append([len(chunks)] + op)
            # (each call runs a frame: the first ones' frames mix nothing new before the last)
            for k, op in enumerate(todo):
                if op[0] == 'song':
                    h.call(PLAY_SONG, PLAYER, SONG_TABLE + 20 * op[1])
                elif op[0] == 'sfx':
                    h.call(PLAY_SFX, VOICES, SFX_TABLE + 12 * op[1], op[2], op[3] & 0xFFFFFFFF)
                elif op[0] == 'stop_sfx':
                    h.call(STOP_ALL_SFX, VOICES)
                elif op[0] == 'stop':
                    e.w8(PLAYER + 5, 0)
                    e.run(1)
                elif op[0] == 'clear':
                    h.call(CLEAR_TRACKS, PLAYER)
                elif op[0] == 'volm':
                    h.call(0x0803F18C, PLAYER, op[1])
                elif op[0] == 'vols':
                    h.call(0x0803E42A, VOICES, op[1])
                assert len(todo) == 1, 'one op per frame'
        else:
            e.run(1)
        new = e.r32(SOUND + 8) & 0x3FF
        n = (new - pos) & 0x3FF
        left = e.read(0x02000000, 0x400)
        right = e.read(0x02000400, 0x400)
        for k in range(n):
            p = (pos + k) & 0x3FF
            out.append(left[p])
            out.append(right[(p + 0x200) & 0x3FF])
        chunks.append(n)
        pos = new
    (OUT / f'trans_{name}.bin').write_bytes(bytes(out))
    (OUT / f'trans_{name}.json').write_text(json.dumps(dict(st, chunks=chunks, ops=ops)))
    h.silence()
    print(name, len(out) // 2, 'samples', len(chunks), 'chunks', flush=True)


def main():
    h = Harness()
    h.e.set_volume(0)
    for n in (sys.argv[1:] or list(SEQS)):
        run(h, n, SEQS[n])


if __name__ == '__main__':
    main()
