"""Player attacks: melee, block, skills (energy attacks / transformations) -> data/buu/attacks_detail.json

Static values are read from the ROM (addresses given per field); measured values come from the
emulator probes atk_batch.py (work/buu/atk_measured.json) and atk_melee.py (work/buu/atk_melee.json).
Every field group carries 'status': 'verified' (read from code/table and confirmed in the emulator),
'measured' (emulator only) or 'inferred' (read from code, not exercised).
"""
import json
from collections import defaultdict
from common import *
from strtab import S
from vtab import entries

SKILL_TAB = 0x0871DAF8      # 24 B/skill: +0 u32 max level index, +0xC u16 name string
ATK_TYPES = 0x086B8088      # 8 B: u8 kind (0 melee, 1 energy), u8 sub, u16 0, u32 power
COSTS = 0x0871DA98
BEAM_DEFS = 0x0871DA68      # 16 B per level
BEAM_DRAIN = 0x0871E148     # u32 per level, EP*256 per frame
BIGBANG_RATE = 0x0871DDBC   # u32 per level, EP*256 per frame of charge
MELEE_RECT = 0x083C344C     # 4 x (s32 x0, y0, x1, y1) by facing
KI_DIR = 0x08719C40         # 4 x (s32 vx, vy) in 1/256 px
FACING = ['down', 'up', 'left', 'right']

STATE_VT = {1: 0x0804DE7C, 2: 0x0804D05C, 3: 0x0804E244, 4: 0x0804DF18, 5: 0x0804D9B0, 6: 0x0804E2D8,
            7: 0x0804DC14, 8: 0x0804DC38, 9: 0x0804DC5C, 10: 0x0804D9D4, 11: 0x0804E088, 13: 0x0804DC80,
            15: 0x0804D9F8, 16: 0x0804DA1C, 17: 0x0804D080, 18: 0x0804D410, 23: 0x0804DFE8}


def cost_bytes(off):
    return [u8(COSTS + off + i) for i in range(3)]


def cost_u32(off):
    return [u32(COSTS + off + 4 * i) for i in range(3)]


# EP cost per level (before the record's cost factor rec+0x56 /16). Source: cost table / immediates.
COST = {
    2: dict(levels=cost_bytes(0x00), addr=hex(COSTS + 0x00), note='minimum to start; charge spends more'),
    6: dict(levels=cost_bytes(0x03), addr=hex(COSTS + 0x03), note='minimum to start; beam drains while held'),
    10: dict(levels=cost_bytes(0x06), addr=hex(COSTS + 0x06), note='minimum to start'),
    11: dict(levels=cost_bytes(0x09), addr=hex(COSTS + 0x09), note='minimum to start; beam drains while held'),
    15: dict(levels=cost_bytes(0x0C), addr=hex(COSTS + 0x0C), note='minimum to start; beam drains while held'),
    16: dict(levels=cost_bytes(0x0F), addr=hex(COSTS + 0x0F), note='minimum to start; beam drains while held'),
    4: dict(levels=cost_u32(0x18), addr=hex(COSTS + 0x18), note='paid on press'),
    8: dict(levels=cost_u32(0x30), addr=hex(COSTS + 0x30), note='paid on press'),
    17: dict(levels=cost_u32(0x48), addr=hex(COSTS + 0x48), note='paid on press'),
    1: dict(fixed=200, note='minimum to start; drains ~3.6 EP/frame while held'),
    3: dict(fixed=12, note='minimum 12 to start, 2 per volley'),
    5: dict(fixed=300, note='paid on press'),
    7: dict(fixed='5% of max EP', note='ceil(maxEP*0xCCD/65536) per shot'),
    9: dict(fixed=350, note='minimum; drains ~3.3 EP/frame while charging'),
    13: dict(fixed=160, note='paid on press'),
    14: dict(fixed=40),
    20: dict(fixed=10, note='then runs on the SS gauge (rec+0x1E), 1 per 20 frames'),
    23: dict(fixed=350, note='paid on press'),
}

KIND = {  # what the skill does, from its state class (atk_states.txt) and measurements
    1: 'beam (hold B)', 2: 'charged ball (hold B, release to fire, >=60 frames or it fizzles)',
    3: 'rapid ki volley (hold B)', 4: 'single projectile', 5: 'dash-through rush (player becomes the hitbox)',
    6: 'beam (hold B)', 7: 'ki blast projectile (refire with B after 40 frames)', 8: 'short-range multi-hit burst',
    9: 'charge (hold B) then single melee-range slash on release', 10: 'instant full-screen beam',
    11: 'beam (hold B), short range', 12: 'Fusion Dance (needs partner)', 13: 'Kamikaze Ghost (homing ghosts)',
    14: 'Instant Transmission (teleport)', 15: 'beam (hold B)', 16: 'beam (hold B)',
    17: 'charged blast, fires after a fixed wind-up', 18: 'transform -> Great Saiyaman (form 14)',
    20: 'transform -> Super Saiyan', 22: 'transform -> Super Saiyan 3', 23: 'thrown ball, status effect',
}


def attack_types():
    out = []
    for i in range(0x14):
        a = ATK_TYPES + 8 * i
        k, sub, pad, pw = u8(a), u8(a + 1), u16(a + 2), u32(a + 4)
        if k > 1 or pad:
            break
        out.append(dict(addr=hex(a), kind=['melee', 'energy'][k], sub=sub, power=pw))
    return out


SUB_NAMES = {2: 'melee punch', 3: 'melee (cannot crit)', 4: 'Energy Blast', 5: 'Kamehameha', 6: 'Electric Kamehameha',
             7: 'Masenkoha', 8: 'Kamekameha', 9: 'Energy Punch', 10: 'Burning Attack', 11: 'Final Flash',
             12: 'Flame Kamehameha', 13: 'Big Bang', 14: 'Banshee Blast', 15: 'Energy Sword',
             16: 'Ultra Volleyball Attack', 17: 'Big Bang Kamehameha', 18: 'Dragon Fist Explosion'}   # game skill names


def beam_defs():
    out = []
    for lv in range(3):
        a = BEAM_DEFS + 16 * lv
        out.append(dict(addr=hex(a), level=lv, w=u8(a), h=u8(a + 1), mult=u16(a + 2), unk4=u16(a + 4),
                        attack_sub=u16(a + 6), gfx=hex(u32(a + 8)), anim=hex(u32(a + 12)),
                        drain_ep_per_frame=round(u32(BEAM_DRAIN + 4 * lv) / 256, 3)))
    return out


def summarize(runs):
    """Collapse atk_measured.json rows to per (skill, level)."""
    by = defaultdict(list)
    for r in runs:
        by[(r['skill'], r['level'])].append(r)
    out = {}
    for (sk, lv), rs in by.items():
        trials = []
        for r in rs:
            h = r['hits']
            trials.append(dict(hold=r['hold'], dist=r['dist'], press_cost=r['cost_on_press'], ep_total=r['ep_total'],
                               idle_frame=r['idle_frame'], hits=len(h), first_hit=h[0][0] if h else None,
                               last_hit=h[-1][0] if h else None, mults=sorted({x[3] for x in h}),
                               subs=sorted({x[2] for x in h}), form_after=r['form_after']))
        out[(sk, lv)] = trials
    return out


def melee_forms():
    p = WORK / 'atk_melee.json'
    if not p.exists():
        return []
    out = []
    for d in json.loads(p.read_text()):
        if not d.get('melee'):
            continue
        lens = sorted({n for v in d['melee'].values() for n in v})
        ok = d['rearm_frames'] == 17 and lens == [17]
        out.append(dict(form=d['form'], name=d['name'], melee_anims=[k for k in d['melee'] if k != '0x0'],
                        recovery_frames=17 if ok else None, walk_256ths=d['dash']['walk'],
                        dash_256ths=d['dash']['dash'],
                        status='measured' if ok else 'probe failed (form not usable from this warp state)'))
    return out


def main():
    runs = json.loads((WORK / 'atk_measured.json').read_text())
    meas = summarize(runs)
    skills = []
    for sk in range(1, 0x18):
        a = SKILL_TAB + 24 * sk
        name = S(u16(a + 12))
        if not name:
            continue
        mx = u32(a)
        d = dict(id=sk, name=name, table_addr=hex(a), raw=rom()[ra(a):ra(a) + 24].hex(), max_level=mx,
                 kind=KIND.get(sk), cost=COST.get(sk))
        if sk in STATE_VT:
            vt = STATE_VT[sk]
            d['state_class'] = dict(vtable=hex(vt), fns=[hex(f) for f in entries(vt, 5)],
                                    slots='enter, update, input, destroy, misc', status='verified')
        levels = []
        for lv in range(3 if mx == 2 else 1):
            t = meas.get((sk, lv))
            if t:
                levels.append(dict(level=lv, trials=t, status='measured',
                                   setup='Goku form 2, maxEP 999, enemy pinned 40/100 px in front, hold B 3/60/150 f'))
        d['measured'] = levels
        skills.append(d)

    rects = [dict(facing=FACING[i], x0=s32(MELEE_RECT + 16 * i), y0=s32(MELEE_RECT + 16 * i + 4),
                  x1=s32(MELEE_RECT + 16 * i + 8), y1=s32(MELEE_RECT + 16 * i + 12)) for i in range(4)]
    ki = [dict(facing=FACING[i], vx=s32(KI_DIR + 8 * i) / 256, vy=s32(KI_DIR + 8 * i + 4) / 256) for i in range(4)]

    out = dict(
        melee=dict(
            fn='0x08012A24', rect_table=hex(MELEE_RECT), rects_rel_player=rects, hit_mult=0x400,
            attack_type='0x086B8098 (melee sub 2)', next_state='4 + rand(3) (three swing animations, same logic)',
            recovery_frames=17, status='verified',
            note='One instant hitbox per swing, same rect for every form; a new A press is accepted once back in idle.',
            forms=melee_forms()),
        block=dict(
            button='R', state=7, damage_factor='obj+0x1C0 = 0x1000 -> 1/16 damage', frontal_only=True,
            status='verified'),
        energy_block=dict(
            button='R+B', fn='0x0801DFE0', update='0x08012BC6',
            drain='acc += rec[+0x57]*0x3277>>4 per frame, spend acc>>16 (~0.197 EP/frame at factor 16)',
            ends='B released (back to block) / R released / EP 0', status='inferred'),
        dash=dict(field_walk='player+0x238', field_dash='player+0x23C', formula='walk 5*SPD+102, dash +320 (SS +128)',
                  units='1/256 px per frame', status='measured'),
        attack_types=attack_types(), attack_sub_names=SUB_NAMES,
        hit_struct={'+0x00': 'u32 direction', '+0x04': 'u32 damage multiplier (0x400 = 1.0)',
                    '+0x08': 'attack-type* (0x086B8088 table)', '+0x0C': 'attacker obj*', '+0x10': 'crit flag',
                    '+0x14': 'raw damage', '+0x18': 'final damage'},
        ki_blast=dict(ctor='0x080197B4', dir_table=hex(KI_DIR), velocity_px_per_frame=ki, mult=1024,
                      cost='5% max EP', refire='B again once state timer >= 40', status='verified'),
        beams=dict(ctor='0x080206B8', obj_size=0x54, obj_vtable='0x0804AEF8', dir_table='0x0871E0F8',
                   level_defs=beam_defs(), drain_table=hex(BEAM_DRAIN), fire_frame=24,
                   drain_formula='acc += drain[lv]*rec[+0x56]/16; SpendEP(acc>>8); acc &= 0xFF',
                   used_by=[15, 16], status='verified'),
        big_bang=dict(enter='0x0801A7C0', ball_ctor='0x08037480', ball_defs='0x0876ACC0',
                      charge_rate=[round(u32(BIGBANG_RATE + 4 * i) / 256, 3) for i in range(3)],
                      charge_cap=1000, min_age_frames=60, mult='~0.35 x EP charged', ball_speed_px_per_frame=0.58,
                      status='measured'),
        skills=skills,
    )
    (DATA / 'attacks_detail.json').write_text(json.dumps(out, indent=1))
    print(len(skills), 'skills,', len(out['attack_types']), 'attack types,', len(out['melee']['forms']), 'forms')


if __name__ == '__main__':
    main()
