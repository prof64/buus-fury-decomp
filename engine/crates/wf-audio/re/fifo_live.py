"""Research/validation only: capture the exact 8-bit FIFO stream of the RUNNING game (not the frozen harness of
fifo_trans.py) over stretches where its main loop's iterations are long - the pause menu opened over music, moved
in, closed; an item picked up (the item-get presentation) - with the game's own sound calls.

The game's sound update (0x0803F296, from the main loop after the state update - not an IRQ) runs once per main-loop
iteration: n = (the DMA's position - the write position) & 0x3FF; under 64 nothing; else the SFX voices' pending
changes once, then the music and the SFX mixed for all n samples (in runs of at most 512, the mix buffer's size). So a
call mixes whatever the DMA has played since the last one. The timer interrupt that counts the DMA's position
(0x03000AE0, every 16 samples) calls the same update itself (0x03000B58) when only 96 samples are left unplayed and
the main loop's update is not running: under a long iteration (a menu's redraw: several video frames) the ring is
refilled from the interrupt, 928 samples at a time, and the iteration's own update follows with the rest. The calls
an iteration made before such an interrupt take effect at it (the SFX voices' pending changes are applied by every
update). The capture logs, in order, every
sound call (breakpoints) and every update (a write of the ring's write position) with the samples it mixed.

Nothing is injected: at the start the driver's whole RAM is taken (between two updates) and the Rust driver starts
from it (`Driver::seed_ram`). Output
work/audio/fifo_ref/live_NAME.bin (interleaved L,R s8) and data/audio/live_NAME.json: `ram`, `chunks` (samples per
update), `ops` [[chunk index, op, args...]] as fifo_trans.py, `frames` (video frames), `long` (the largest chunk),
`irq` (updates run from the interrupt), `samples` and `fnv` (FNV-1a 64 of the stream). Checked by tests/fifo_exact.rs
`live_game_matches_fifo_capture`.

    .venv/bin/python engine/crates/wf-audio/re/fifo_live.py [NAME ...]
"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from fifo_ref import ROOT, OUT, state  # noqa: E402
from buu_sound import SOUND, PLAYER, VOICES, PLAY_SONG, PLAY_SFX, SONG_TABLE, SFX_TABLE, CLEAR_TRACKS, STOP_ALL_SFX  # noqa: E402
sys.path.insert(0, str(ROOT / 're' / 'buu'))
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'menu'))
from cap import program  # noqa: E402
from common import emu  # noqa: E402

UPDATE = 0x0803F296            # the main loop's sound update
STOP_SFX, STOP_MATCH = 0x0803E302, 0x0803E32C
VOL_MUSIC, VOL_SFX = 0x0803F18C, 0x0803E42A

# NAME: (state, keys before the capture, keys captured)
SEQS = {
    # START over the area's music: the pause menu's open (a long iteration), the cursor, L / R pages, B close
    'pause_menu': ('buu_snakeway_start.ss', ['-:10'],
                   ['-:120', 'START:2', '-:90', 'DOWN:2', '-:20', 'DOWN:2', '-:20', 'UP:2', '-:30', 'R:2', '-:60', 'R:2',
                    '-:60', 'L:2', '-:60', 'A:2', '-:40', 'B:2', '-:40', 'START:2', '-:200']),
    # the pause menu's other pages: R through them with A / the arrows on each (an item's Use, the Options' sliders -
    # the volumes change under the music), then closed
    'pause_pages': ('buu_snakeway_start.ss', ['-:10'],
                    ['-:60', 'START:2', '-:90', 'R:2', '-:70', 'A:2', '-:50', 'A:2', '-:70', 'R:2', '-:70', 'R:2', '-:70',
                     'LEFT:2', '-:20', 'LEFT:2', '-:20', 'DOWN:2', '-:20', 'LEFT:2', '-:20', 'RIGHT:2', '-:20', 'UP:2',
                     '-:20', 'RIGHT:2', '-:20', 'RIGHT:2', '-:40', 'START:2', '-:150']),
}


DATA = ROOT / 'data' / 'audio'
DATA.mkdir(parents=True, exist_ok=True)


def fnv(b):
    h = 0xcbf29ce484222325
    for x in b:
        h = ((h ^ x) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h


def s8(v):
    return v - 256 if v > 127 else v


def leg(chain, row, lead=200, frames=700):
    """a stretch of a recorded real run (tools/wholerun: the game from its reset with data/realrunN/*.keys.gz): the
    keys up to `lead` frames before video row `row` (a sound request's row in sounds.json.gz), then `frames` captured"""
    sys.path.insert(0, str(ROOT / 'tools' / 'wholerun'))
    sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
    sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
    import chains
    keys = [k for _, ks, _ in chains.parts(chain) for k in ks]
    return 'reset', keys[:row - lead], keys[row - lead:row - lead + frames]


def run(name, st_file, pre, keys):
    if st_file == 'reset':
        import route
        e = route.start_emu('reset')
    else:
        e = emu(st_file)
    e.set_volume(0)
    pre = program(pre) if pre and isinstance(pre[0], str) else pre
    keys = iter(program(keys) if keys and isinstance(keys[0], str) else keys)
    for k in pre:
        e.set_keys(k)
        e.run(1)
    start(e, name, keys)


def start(e, name, keys, feed=None, step=None):
    """the capture proper, from the emulator as it stands (`feed` / `step`: a video frame's keys and the frame itself -
    a linked pair's)"""
    step = step or (lambda: e.run(1))
    feed = feed or e.set_keys
    song = (e.r32(PLAYER + 0xC) - SONG_TABLE) // 20
    # (a video frame's end inside an update - the busy flag, SOUND + 0 - is no place to take the driver's RAM)
    while e.r32(SOUND) & 0xFF:
        feed(next(keys))
        step()
    bps = {e.breakpoint(a): a for a in (PLAY_SFX, STOP_SFX, STOP_ALL_SFX, STOP_MATCH, PLAY_SONG, CLEAR_TRACKS, VOL_MUSIC, VOL_SFX)}
    # (an update = a write of the ring's write position, SOUND + 8, at its start: old -> new, n = the difference)
    wp = e.watch(SOUND + 8, SOUND + 12, 'w')
    # (... and the ring's own writes, so each update's samples are taken as it wrote them: two updates of one video
    # frame - the interrupt's and the main loop's - can write the same slots)
    wr = e.watch(0x02000000, 0x02000800, 'w')
    shadow = bytearray(e.read(0x02000000, 0x800))
    e.hits()
    # the whole driver as it stands - the player's header, its 15 tracks, the 16 SFX voices (`Driver::seed_ram`)
    st = dict(state(e), ram=e.read(PLAYER, 0x18 + 0x60 * 15 + 0x40 * 16).hex())
    out = bytearray()
    chunks, ops, frames = [], [], 0
    irq = [0]
    playing = e.r8(PLAYER + 5)

    cur = [None]

    def close():
        """the running update's samples, from the ring as written so far"""
        if cur[0] is None:
            return
        pos, n = cur[0]
        cur[0] = None
        for k in range(n):
            p = (pos + k) & 0x3FF
            out.append(shadow[p])
            out.append(shadow[0x400 + ((p + 0x200) & 0x3FF)])
        chunks.append(n)

    def take(hits):
        """a video frame's hits in order: the calls, the updates and the ring's writes"""
        nonlocal playing
        for h in hits:
            if h['kind'] == 'watch':
                a = h['addr']
                if 0x02000000 <= a < 0x02000800:
                    for i in range(h['width']):
                        shadow[a - 0x02000000 + i] = (h['new'] >> (8 * i)) & 0xFF
                    continue
                close()
                n = (h['new'] - h['old']) & 0x3FF
                if n:
                    cur[0] = (h['old'] & 0x3FF, n)
                    # (called from the timer interrupt 0x03000B58 - the ring about to run out under a long iteration)
                    irq[0] += (h['lr'] >> 24) == 3
                continue
            if h['kind'] != 'break':
                continue
            # (a call inside an update's span cannot be: the update runs to its end; a call after it is the next one's)
            close()
            c = len(chunks)
            a, r = h['addr'] & ~1, h['regs']
            if a == PLAY_SFX:
                ops.append([c, 'sfx', (r[1] - SFX_TABLE) // 12, r[2] & 0xFF, s8(r[3] & 0xFF)])
            elif a == STOP_SFX:
                ops.append([c, 'stop_one', (r[1] - SFX_TABLE) // 12])
            elif a == STOP_MATCH:
                ops.append([c, 'stop_match', (r[1] - SFX_TABLE) // 12])
            elif a == STOP_ALL_SFX:
                ops.append([c, 'stop_sfx'])
            elif a == PLAY_SONG:
                ops.append([c, 'song', (r[1] - SONG_TABLE) // 20])
            elif a == CLEAR_TRACKS:
                ops.append([c, 'clear'])
            elif a == VOL_MUSIC:
                ops.append([c, 'volm', r[1] & 0xFF])
            elif a == VOL_SFX:
                ops.append([c, 'vols', r[1] & 0xFF])
        p5 = e.r8(PLAYER + 5)
        if playing and not p5:
            close()
            ops.append([len(chunks), 'stop'])
        playing = p5

    def hits():
        import ctypes
        from emu import Hit
        ptr = ctypes.POINTER(Hit)()
        dropped = ctypes.c_uint32()
        n = e.L.emu_hits(e.h, ctypes.byref(ptr), ctypes.byref(dropped))
        assert dropped.value == 0, f'{dropped.value} hits dropped'
        hs = [ptr[i].as_dict() for i in range(n)]
        e.L.emu_clear_hits(e.h)
        return hs

    for k in keys:
        feed(k)
        step()
        frames += 1
        take(hits())
    # (the last update is whole once the next frame ran)
    step()
    last = []
    for h in hits():
        if h['kind'] == 'watch' and not 0x02000000 <= h['addr'] < 0x02000800:
            break
        if h['kind'] == 'watch':
            last.append(h)
    take(last)
    close()
    for b in list(bps) + [wp, wr]:
        e.clear(b)
    (OUT / f'live_{name}.bin').write_bytes(bytes(out))
    doc = dict(ram=st['ram'], chunks=chunks, ops=ops, frames=frames, long=max(chunks), irq=irq[0], song=song,
               samples=len(out) // 2, fnv=f'{fnv(out):016x}')
    (OUT / f'live_{name}.json').write_text(json.dumps(doc))
    # the committed reference (no sound data: the driver's RAM at the start, the updates, the calls, the stream's
    # FNV-1a 64) - tests/fifo_exact.rs `live_game_matches_fifo_capture`
    (DATA / f'live_{name}.json').write_text(json.dumps(doc, separators=(',', ':')))
    print(name, len(out) // 2, 'samples', len(chunks), 'updates in', frames, 'frames; largest chunk', max(chunks), 'from the interrupt', irq[0], 'ops', len(ops), flush=True)
    return chunks, ops


# NAME: (chain, video row) - items picked up in a recorded real run (run 2, rows of PlaySfx 45 in
# data/realrun2/sounds.json.gz): the item-get presentation (the field held 61 frames) over the area's music; two in a
# row in a house, one in the open
LEGS = {
    'item_get': ('run2', 62007, 200, 700),
    'item_get_field': ('run2', 75756, 200, 600),
}


def link(name, rec, at_sfx, lead=200, frames=700):
    """a recorded link fight (tools/link/dojo_record.py: data/link/dojo_REC.json, its `video_keys` on two linked
    emulators from their boot): the host's sound from `lead` frames before its first PlaySfx(at_sfx) - the prize's
    supply picked up (the link fight's item-get) over the Dojo's song"""
    sys.path.insert(0, str(ROOT / 'tools' / 'link'))
    import dojo_record as dr
    vk = json.loads((ROOT / 'data' / 'link' / f'dojo_{rec}.json').read_text())['video_keys']

    def pair():
        sav = (ROOT / 'work' / 'run2' / 'end.sav').read_bytes()
        a, b = dr.mkemu('buu_menus.ss'), dr.mkemu('buu_menus.ss')
        for x in (a, b):
            dr.sbemu.sd_write(x, sav)
            x.run(3)
            dr.soft_reset(x)
        dr.L.emu_link(a.h, 0)
        dr.L.emu_link(b.h, 1)
        return a, b

    def feed_of(a, b):
        def feed(k):
            a.set_keys(k[0])
            b.set_keys(k[1])
        return feed

    a, b = pair()
    bp = a.breakpoint(PLAY_SFX)
    a.hits()
    at = None
    for f, k in enumerate(vk):
        feed_of(a, b)(k)
        dr.step(a, b)
        if any(h['kind'] == 'break' and (h['regs'][1] - SFX_TABLE) // 12 == at_sfx for h in a.hits()):
            at = f
            break
    assert at is not None and at > lead, f'no PlaySfx({at_sfx}) in dojo_{rec}'
    print('PlaySfx', at_sfx, 'at video frame', at, flush=True)
    # (a second pair in one process fails: the capture runs in a process of its own - `--link-at`)
    import subprocess
    subprocess.run([sys.executable, __file__, name, '--link-at', str(at)], check=True)


def link_capture(name, rec, at, lead=200, frames=700):
    sys.path.insert(0, str(ROOT / 'tools' / 'link'))
    import dojo_record as dr
    vk = json.loads((ROOT / 'data' / 'link' / f'dojo_{rec}.json').read_text())['video_keys']
    sav = (ROOT / 'work' / 'run2' / 'end.sav').read_bytes()
    a, b = dr.mkemu('buu_menus.ss'), dr.mkemu('buu_menus.ss')
    for x in (a, b):
        dr.sbemu.sd_write(x, sav)
        x.run(3)
        dr.soft_reset(x)
    dr.L.emu_link(a.h, 0)
    dr.L.emu_link(b.h, 1)

    def feed(k):
        a.set_keys(k[0])
        b.set_keys(k[1])
    for k in vk[:at - lead]:
        feed(k)
        dr.step(a, b)
    a.set_volume(0)
    rest = vk[at - lead:at - lead + frames] + [[0, 0]] * frames
    start(a, name, iter(rest[:frames]), feed=feed, step=lambda: dr.step(a, b))


# NAME: (recording, the SFX) - the link fight's supply pick-up
LINKS = {
    'link_item_get': ('c9', 45),
}


def main():
    if '--link-at' in sys.argv:
        n = sys.argv[1]
        return link_capture(n, LINKS[n][0], int(sys.argv[sys.argv.index('--link-at') + 1]))
    for n in (sys.argv[1:] or list(SEQS) + list(LEGS) + list(LINKS)):
        if n in LINKS:
            # (the linked pair's library is loaded first: a process of its own)
            import subprocess
            if len(sys.argv) == 2:
                link(n, *LINKS[n])
            else:
                subprocess.run([sys.executable, __file__, n], check=True)
        elif n in LEGS:
            run(n, *leg(*LEGS[n]))
        else:
            run(n, *SEQS[n])


if __name__ == '__main__':
    main()
