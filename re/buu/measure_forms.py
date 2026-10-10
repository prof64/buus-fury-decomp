"""For every playable form (char table 0x086B82EC entries 0..53): make it the active character's form,
re-create the player via warp, and measure stat/speed bonuses applied to the player object relative
to the record (VERIFIED by emu). Writes work/buu/forms_measured.json."""
import json
from common import *
from warp import warp
from strtab import S

REC0 = 0x03001560
GD = 0x03001068
CT = 0x086b82ec
out = []
for form in range(54):
    ent = CT + 24 * form
    ci = u8(ent + 0xc)
    if ci > 8:
        continue
    e = emu('buu_snakeway_start.ss')
    rec = REC0 + 0x68 * ci
    # make record usable: copy Goku's stats so comparisons are uniform
    e.w8(GD, ci)
    e.w8(rec + 0x14, form)
    for k, v in ((0x4c, 50), (0x4e, 60), (0x50, 70), (0x52, 80), (0x54, 50)):
        e.w16(rec + k, v)
    e.w16(rec + 0x16, 500); e.w16(rec + 0x18, 500); e.w16(rec + 0x1a, 300); e.w16(rec + 0x1c, 300)
    e.w16(rec + 0x1e, 300); e.w16(rec + 0x20, 300)
    try:
        ok = warp(e, 8, 3, 0x80, 0x80, frames=240)
    except AssertionError as ex:
        ok = False
    p = e.r32(0x03001c34)
    if not ok or not p:
        out.append(dict(form=form, ok=False))
        print(form, 'warp failed')
        continue
    st = [e.r16(p + 0x1b0 + 2 * i) for i in range(4)]
    walk, dash = e.r32(p + 0x238), e.r32(p + 0x23c)
    rec2 = e.read(rec, 0x68)
    rs = [e.r16(rec + 0x4c + 2 * i) for i in range(5)]
    d = dict(form=form, ci=ci, name=S(u16(ent + 2)), ok=True, obj_vt=hex(e.r32(p)), form_after=rec2[0x14],
             stats_obj=st, stats_rec=rs, stat_bonus=[st[i] - rs[i] for i in range(4)],
             walk=walk, dash=dash, walk_bonus=walk - (5 * rs[4] + 102), dash_bonus=dash - (5 * rs[4] + 102 + 320),
             resist=[e.r8(p + 0x1b8), e.r8(p + 0x1b9)])
    out.append(d)
    print(d)
(WORK / 'forms_measured.json').write_text(json.dumps(out, indent=1))
