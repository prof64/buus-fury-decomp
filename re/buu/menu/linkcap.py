"""linkcap.py -- record the link-cable screens (Dojo / Trading Post) from two linked emulators as menu traces
(headless, muted), the ground truth engine/crates/wf-app/tests/menu_parity.rs gates them against.

    .venv/bin/python re/buu/menu/linkcap.py [scenario ...]     (default: all in scenarios_link.LINK)

Both games boot with run 2's save (work/run2/end.sav) and are wired by the link cable (tools/emu shim `emu_link`,
mGBA's lockstep coordinator). A scenario: `flow` - the two sides' keys up to the moment recorded (title ->
Multiplayer -> ...), then `keys` / `keys_b` (key programs as capture.py's, B's padded with nothing) recorded per
video frame of A (the host, player 1): keys, sound calls, the screen and its sha1, the mode loop's counter and mode -
written as capture.py writes a scenario (work/buu/menu/<name>/fNNNN.png - ROM-derived, git-ignored - and
data/menu/<name>.json with `meta.open`: what the remake opens at frame 0)."""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'link'))
import dojo_record as dr  # noqa: E402  (the savedata + link emulator library, loaded first)
sys.path.insert(0, str(HERE))
from cap import Cap, program, OUT  # noqa: E402
import capture  # noqa: E402
import numpy as np  # noqa: E402


class LinkCap(Cap):
    """Cap over emulator A, stepping the linked pair (B's keys alongside)"""

    def __init__(self, a, b):
        super().__init__(e=a)
        self.b = b
        self.keys_b = 0

    def step(self, keys, probe=None, keep=True):
        import hashlib
        self.e.set_keys(keys)
        self.b.set_keys(self.keys_b)
        dr.step(self.e, self.b)
        px = self.e.pixels()
        rgb = np.frombuffer(px, dtype=np.uint8).reshape(160, 240, 3).copy()
        row = dict(keys=keys, snd=self.snd(), h=hashlib.sha1(rgb.tobytes()).hexdigest()[:16])
        if probe:
            row.update(probe(self.e))
        self.frames.append(row)
        if keep:
            self.screens.append(rgb)
        return row


def pair():
    sav = (ROOT / 'work' / 'run2' / 'end.sav').read_bytes()
    a, b = dr.mkemu('buu_menus.ss'), dr.mkemu('buu_menus.ss')
    for e in (a, b):
        dr.sbemu.sd_write(e, sav)
        e.run(3)
        dr.soft_reset(e)
    dr.L.emu_link(a.h, 0)
    dr.L.emu_link(b.h, 1)
    return a, b


def run(name, sc):
    a, b = pair()
    c = LinkCap(a, b)
    for ka, kb, n in sc['flow']:
        for _ in range(n):
            c.keys_b = dr.keyof(kb)
            c.step(dr.keyof(ka), keep=False)
    st = capture.story(a)
    # (as the title scenarios: the save slots from the game's EEPROM cache, and the running mode's fields the page
    # starts from - `sc['probe0'](e, meta_open)` fills page-specific ones)
    from scenarios_title import slot_b
    base = a.r32(0x03004A64)
    saves = [slot_b(a.read(base + 0x10 + 0x8B0 * k, 0x8B0)) for k in range(3)]
    if sc.get('probe0'):
        sc['probe0'](a, sc['open'])
    c.frames, c.screens = [], []
    ka, kb = program(sc['keys']), program(sc.get('keys_b', []))
    # (a scenario's own per-frame probe of emulator A, merged into the row; `post(frames, rows)` turns it into what
    # the rows carry for the remake, e.g. the friend's side of the trade: `link`)
    probe = capture.probe
    if sc.get('probe'):
        probe = lambda e: {**capture.probe(e), **sc['probe'](e)}  # noqa: E731
    for i, k in enumerate(ka):
        c.keys_b = kb[i] if i < len(kb) else 0
        # (the link's up flag [0x03002F54] per frame: the title's link box counts from it)
        c.step(k, lambda e: dict(probe(e), up=e.r32(0x03002F54)))
    d = OUT / name
    d.mkdir(parents=True, exist_ok=True)
    from PIL import Image
    for f in d.glob('f*.png'):
        f.unlink()
    for i, s in enumerate(c.screens):
        Image.fromarray(s).save(d / f'f{i:04d}.png')
    meta = {'open': sc['open'], 'link': True, 'saves': saves}
    rows = [dict(keys=f['keys'], lf=f['lf'], snd=f['snd'], h=f['h'], **({'up': f['up']} if sc.get('up') else {}))
            for f in c.frames]
    if sc.get('post'):
        sc['post'](c.frames, rows)
    capture.DATA.mkdir(parents=True, exist_ok=True)
    (capture.DATA / f'{name}.json').write_text(json.dumps(dict(scenario=name, state='link', meta=meta, story=st, frames=rows),
                                                          separators=(',', ':')))
    print(name, len(rows), 'frames ->', d)


if __name__ == '__main__':
    from scenarios_link import LINK
    for n in (sys.argv[1:] or list(LINK)):
        run(n, LINK[n])
