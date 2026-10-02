"""Per playable form: melee attack state length (A press -> idle), the 3 random melee animations,
earliest frame a second A press starts a new swing, and dash speed/duration.
Writes work/buu/atk_melee.json."""
import json
from collections import defaultdict
from common import *
from warp import warp
from strtab import S

REC0 = 0x03001560
GD = 0x03001068
CT = 0x086B82EC
IDLE = 0x03002B44
PLAYER = 0x03001c34


def make(form):
    ci = u8(CT + 24 * form + 0xc)
    e = emu('buu_snakeway_start.ss')
    rec = REC0 + 0x68 * ci
    e.w8(GD, ci)
    e.w8(rec + 0x14, form)
    e.w16(rec + 0x16, 500); e.w16(rec + 0x18, 500); e.w16(rec + 0x1a, 300); e.w16(rec + 0x1c, 300)
    e.w16(rec + 0x1e, 300); e.w16(rec + 0x20, 300)
    ok = warp(e, 8, 3, 0x80, 0x80, frames=240)
    return e, ci, ok


def wait_idle(e, p, n=120):
    for _ in range(n):
        if e.r32(p + 0x14) == IDLE:
            return True
        e.run(1)
    return False


def melee_lengths(e, p, trials=24):
    res = defaultdict(list)
    for t in range(trials):
        wait_idle(e, p)
        e.set_keys(KEY.A); e.run(1); e.set_keys(0)
        anim = e.r32(p + 0x50)
        n = 1
        while e.r32(p + 0x14) != IDLE and n < 200:
            e.run(1); n += 1
        res[hex(anim)].append(n)
    return {k: sorted(set(v)) for k, v in res.items()}


def rearm(e, p):
    """Smallest delay d (frames after the first press) at which a second A press yields a second swing."""
    st = e.save()
    best = None
    for d in range(1, 30):
        e.load(st)
        wait_idle(e, p)
        e.set_keys(KEY.A); e.run(1); e.set_keys(0)
        e.run(d - 1)
        a0 = e.r32(p + 0x50)
        swings = 0
        prev_state = e.r32(p + 0x14)
        e.set_keys(KEY.A); e.run(2); e.set_keys(0)
        # a new swing re-enters the attack state from idle: detect idle seen just before
        if prev_state == IDLE:
            best = d
            break
    e.load(st)
    return best


def dash(e, p):
    wait_idle(e, p)
    x0 = e.r32(p + 0x1a0)
    for k in (KEY.RIGHT, 0, KEY.RIGHT):
        e.set_keys(k); e.run(2)
    e.set_keys(KEY.RIGHT)
    xs = []
    for f in range(60):
        xs.append(e.r32(p + 0x1a0)); e.run(1)
    e.set_keys(0)
    v = [xs[i + 1] - xs[i] for i in range(len(xs) - 1)]
    dashf = [i for i, x in enumerate(v) if x == e.r32(p + 0x23c)]
    return dict(dx_trace_256ths=v[:40], dash_frames=len(dashf), walk=e.r32(p + 0x238), dash=e.r32(p + 0x23c))


out = []
for form in range(54):
    ci = u8(CT + 24 * form + 0xc)
    if ci > 8:
        continue
    e, ci, ok = make(form)
    p = e.r32(PLAYER)
    if not ok or not p:
        out.append(dict(form=form, ok=False)); continue
    d = dict(form=form, ci=ci, name=S(u16(CT + 24 * form + 2)), gfx_def=hex(e.r32(p + 0x58)),
             melee=melee_lengths(e, p), rearm_frames=rearm(e, p), dash=dash(e, p))
    out.append(d)
    print(d, flush=True)
(WORK / 'atk_melee.json').write_text(json.dumps(out, indent=1))
