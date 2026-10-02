"""Regenerate data/log2/moves.json from the LoG2 ROM (player side; enemy_attacks left for the enemy agent).

ROM tables parsed: attack table 0x086DB2CC, special-move type table 0x081D9900, projectile/spawn tables.
Behavioural constants are embedded with the ROM address of the instruction that encodes them.
'measured' blocks come from emulator runs (re/log2/pl_battery.py, pl_runall.py; logs in work/log2/pl_bat_*.txt).
"""
import json, os, re, struct
from common import ROM, u8, u16, u32, s32, ROOT, dis
import pl_attacks

ATK = 0x086DB2CC
MOVETYPES = 0x081D9900
STRTAB = 0x080E2840
def cstr(a):
    o = a & 0x1ffffff
    return ROM[o:ROM.index(b'\0', o)].decode('latin1')
def S(i): return cstr(u32(STRTAB + 4 * i))

def attack_table():
    # who references each entry (literal pools in code)
    refs = {}
    for i in range(0, 0x30000, 4):
        v = u32(0x08000000 + i)
        if ATK <= v < ATK + 16 * 8 and (v - ATK) % 8 == 0:
            refs.setdefault((v - ATK) // 8, []).append(hex(0x08000000 + i))
    users = {0: 'generic melee; also the table base the Spirit Bomb indexes (+0x70 -> id14, +0x78 -> id15; 0x080166B4)',
             1: 'generic energy (projectile class near 0x08016214, enemy AI 0x0801A19C/0x0801CE64)', 2: 'player basic melee (A) / enemy',
             3: 'player basic energy blast + enemy blasts', 4: 'charged special melee hits (state 0x27)',
             5: 'Energy Punch (move type 7)', 6: 'Scatter Shot pellets (move type 8)', 7: 'Masenko Ball explosion (move type 4)',
             8: 'Big Bang explosion (move type 0)', 9: 'Burning Attack (move type 1; low damage)',
             10: 'Kamehameha beam head, per frame (move type 3)', 11: 'Special Beam Cannon beam, per frame (move type 9)',
             12: 'Sword Blast sword swing melee part (state 0x32 enter 0x08015478, literal 0x0801549C)', 13: 'Sword Blast (move type 13)',
             14: 'Spirit Bomb explosion (move type 10; reached as table+0x70)', 15: 'zero-damage stun broadcast (Spirit Bomb release, Hercule special)'}
    out = []
    for i in range(16):
        a = ATK + 8 * i
        out.append(dict(id=i, rom=hex(a), type=u8(a), type_name='melee(STR, defender mult+0x163, can crit)' if u8(a) == 0 else 'energy(POW, defender mult+0x164)',
                        id_byte=u8(a + 1), power=u32(a + 4), power_hex=hex(u32(a + 4)),
                        effective_power_at_scale_1024=u32(a + 4), used_by=users.get(i), code_literal_refs=refs.get(i, [])))
    return out

def scan(fn, n=40):
    out = {}
    if not fn: return out
    for line in dis(fn & ~1, n).split('\n'):
        m = re.search(r'cmp r1, #(0x[0-9a-f]+|\d+)', line)
        if m and 'min' not in out: out['min'] = int(m.group(1), 0)
        m = re.search(r'subs r2, #(0x[0-9a-f]+|\d+)', line)
        if m and 'cost' not in out: out['cost'] = int(m.group(1), 0)
        m = re.search(r'movs r1, #(0x[0-9a-f]+|\d+)', line)
        if m and 'state' not in out and 'cost' in out: out['state'] = int(m.group(1), 0)
        if 'pop' in line or ('bx lr' in line and 'min' in out): break
    return out

NAMES = {0: 'Big Bang (Vegeta)', 1: 'Burning Attack (Trunks)', 2: 'Energy blast (all; basic ki blast)',
         3: 'Kamehameha (Goku slot0 / Gohan slot1)', 4: 'Masenko Ball (Gohan)', 5: 'unused (canUse always false)',
         6: 'Hercule special', 7: 'Energy Punch (Vegeta)', 8: 'Scatter Shot (Piccolo)', 9: 'Special Beam Cannon (Piccolo)',
         10: 'Spirit Bomb (Goku)', 11: 'Power-up / transform (Piccolo)', 12: 'Super Saiyan transform (Gohan, Vegeta, Trunks, Goku)',
         13: 'Sword Blast (Trunks)'}
STRINGS = {0: (0xA9, 0xAE), 1: (0xAA, 0xAF), 3: (0xAB, 0xB0), 4: (0xA7, 0xAC), 7: (0xB3, 0xB8), 8: (0xB2, 0xB7),
           9: (0xA8, 0xAD), 10: (0xB5, 0xBA), 13: (0xB4, 0xB9)}

DETAILS = {
 2: dict(player_state=0x22, attack_id=3, scale=1024,
         static='state 0x22 (vt 0x08025D48): anim counter starts 8 (0x08014DEE), +1/frame; projectile spawned when counter%16==0 (first at +8 frames, 0x08014E58); '
                'if B pressed again during the 32-frame window and EP>2: EP-=2, counter=16 -> next shot (rapid fire every 16 f). Projectile ctor 0x0800EAD8, class vt 0x08024CBC',
         measured=dict(first_shot_delay_frames=8, refire_period_frames=16, state_len_frames=24, speed_px_per_frame=3.0,
                       spawn_offset_px=[18, -8], lifetime='until it leaves the camera rect (~40 f at 3 px/f)', hits=1, pierce=False)),
 3: dict(player_state=0x30, attack_id=10, scale=1024,
         static='state 0x30 (vt 0x08025CB8): beam object ctor 0x080143A8 at offset table 0x086E37DC; charge 24 f (0x0801525A); at 24 f fires (0x08014478) only if B still held else cancels; '
                'then EP drain acc += 0x56/frame, EP -= acc>>8 (0x080152A6) = 86/256 EP per frame; ends on B release or EP 0. Beam head extends 3 px/f (0x0801475E) until it touches an enemy/wall/screen edge; '
                'every frame a 16x16 box at the head (0x08014636) is broadcast with attack id 10 power 0x40',
         measured=dict(charge_frames=25, ep_initial=2, ep_drain_per_frame=86 / 256, hits_per_frame='1-2 while touching', damage_per_hit_example='~10 (Goku POW 65 vs snake END 3)')),
 9: dict(player_state=0x31, attack_id=11, scale=1024,
         static='state 0x31 (vt 0x08025F70): same structure as Kamehameha, beam class ctor 0x080157E0 (offset table 0x086E382C), charge 24 f, EP drain acc += 0x22/frame (34/256 EP per frame, 0x080153F8); attack id 11 power 0x66 per frame',
         measured=dict(charge_frames=25, ep_drain_per_frame=34 / 256, damage_per_hit_example='~12-13 (Piccolo POW 55)'),
         notes='Description says the beam passes through enemies (INFERRED, not tested with 2 targets)'),
 4: dict(player_state=0x2D, attack_id=7, scale='min(2*C+256, 2048) where C = clamp(-180 + 30*frames_B_held, 255, 1024)',
         static='state 0x2D (vt 0x08025D5C): enter sets charge=-180 (0x08014F4A), +30 per frame held (0x08014F80); on B release launch 0x08013B12: C=clamp(charge,0xFF,0x400); '
                'vx,vy = dir[facing]*C (dir table 0x086DE8D0) with vy -= 0x200; screen-y gravity +0x20/frame; height h starts 0x800, v_h 0x200, v_h -= 0x20/frame; explodes when h<0 (~36 f after launch): '
                'attack id 7 (power 0x800) over projectile rect with scale = min(2C+256, 0x800) (0x08013B88); spawns explosion 0x080135E0. State ends 32 f after release.',
         measured=dict(tap_scale=766, tap_flight_frames=36, tap_range_px=34, hold60_range_px=136, horizontal_speed_px_per_frame='C/256 (1.0..4.0)')),
 0: dict(player_state=0x2E, attack_id=8, scale='32 + frames charged (ball +0x44 starts 0x20, +1/frame while B held and EP>0)',
         static='state 0x2E (vt 0x080258B4): ball ctor 0x08013E48 (vt 0x080241B0) at offset table 0x086E377C; while held EP-=1 every 4 frames (frame&3==0, 0x080150E0); EP 0 stops growth; '
                'on release 0x08014160: velocity +-0x200 (2 px/f) per facing (0x086DE904) plus an initial 16 px jump; each frame tests a 24x24 box (pos+-12) for any target (msg type 2); '
                'on contact/wall: broadcasts attack id 8 (power 0xC000) over a 64x64 box (pos+-32) with scale = ball+0x44, then 16-frame explosion',
         measured=dict(tap_scale=33, hold90_scale=122, speed_px_per_frame=2.0, initial_jump_px=16, blast_box_px=64, ep_initial=10, ep_drain='1 per 4 frames while held',
                       damage_examples={'tap(scale33)': 246, 'hold90(scale122)': 931})),
 1: dict(player_state=0x2F, attack_id=9, scale=1024,
         static='state 0x2F (vt 0x08025E48): projectile ctor 0x080141B4 (vt 0x0802507C) at offset table 0x086E37AC, hovers while B held; on release 0x08014350: velocity +-0x280 (2.5 px/f) table 0x086E15CC, initial jump 4 frames worth (10 px); '
                'first target touched gets attack id 9 (power 0x40) then the bolt fades (16 f). Low damage; the stun effect is applied by the receiver (enemy AI reaction to attack id 9 - see enemy notes)',
         measured=dict(speed_px_per_frame=2.5, hits=1, damage_example=9)),
 13: dict(player_state=0x32, attack_id=13, scale=1024,
          static='state 0x32 (vt 0x08025CF8, enter 0x08015478): immediate sword-swing melee hit attack id 12 (melee 0x400, scale 0x400) on press, then sword-wave projectile vt 0x08024B24 spawned 8 frames after press (attack id 13)',
          measured=dict(spawn_delay_frames=8, speed_px_per_frame=3.4, state_len_frames=33, damage_example=449, spawn_offset_px=[26, -4])),
 10: dict(player_state=0x33, attack_id=14, scale='5 + frames from press until the throw (throw happens 8 f after B release)',
          static='state 0x33 (vt 0x08025CCC, enter 0x0801565C): bomb ctor 0x080165EC (vt 0x08024AF8) 24 px above head (0x086E3624: +8,-24); size +0x44 starts 5, +1/frame; '
                 'EP-=1 every 4 frames while held (0x080156C6); release: counter 8 then throw 0x080169B8: velocity table 0x086F78F0 (1 px/f), sprite tier min(4,size>>6); '
                 'on detonation broadcasts attack id 14 (power 0xA000) with scale=size, and attack id 15 (power 0) to stun; msg 0x13 broadcast at throw',
          measured=dict(hold40_scale=53, hold200_scale=213, speed_px_per_frame=1.0, ep_initial=10, ep_drain='1 per 4 frames', damage_examples={'scale53': 362, 'scale213': 1483})),
 8: dict(player_state=0x2C, attack_id=6, scale=1024,
         static='state 0x2C (vt 0x08025D88, update 0x08014E98): at anim counter 16 spawns 3 projectiles (0x0801396E -> class ctor 0x080137C4, vt 0x08024D14) from table 0x086E362C (per facing 3 x 24 bytes: dx,dy,vx,vy,param,anim); state 48 f',
         measured=dict(spawn_delay_frames=16, pellets=3, center_speed_px_per_frame=3.0, side_velocity_px_per_frame=[2.6, 1.5], damage_examples=[170, 168, 161])),
 7: dict(player_state=0x2B, attack_id=5, scale=1024,
         static='execute 0x0801330E: EP-=4, immediate melee hit with attack id 5 (power 0x600) over rect table 0x086DB28C[facing] (same as basic melee), anim state 0x2B (16 f)',
         measured=dict(state_len_frames=16, damage_example=151)),
 6: dict(player_state=0x05, attack_id=15, scale=0,
         static='execute 0x0801327C: EP-=6, state 5, broadcasts attack id 15 (power 0) to every entity (0x08006DA6), spawns a 32-frame effect object (vt 0x08025808)',
         measured=dict(effect='0-damage hit on all enemies on the map list (stun/aggro effect handled by enemy side)', state_len_frames=32)),
 12: dict(player_state='0x23 transform (form.transform_to_form) / 0x24 revert if form.flags bit0', attack_id=None,
          static='execute 0x0801353C; canUse 0x080134F2: form.flags bit1 -> unusable; bit0 (already transformed) -> free revert; else needs EP>=10. Transform state 0x23 ~160-175 f (VERIFIED), revert 0x24 ~40-80 f with damage multipliers 0 (invulnerable). Form switch 0x08014D58',
          measured=dict(transform_frames='~165', ep_cost=10)),
 11: dict(player_state='0x25 power-up / 0x24 revert', attack_id=None,
          static='execute 0x080134C0: like type 12 but uses state 0x25 (no SFX, 64-frame anim 0x0801560E) - Piccolo form 6 -> 7',
          measured=dict(transform_frames=64, ep_cost=10)),
}

def move_types():
    out = []
    for i in range(14):
        ic0, ic1, can, ex = struct.unpack_from('<4I', ROM, (MOVETYPES & 0x1ffffff) + 16 * i)
        a = scan(can); b = scan(ex)
        d = dict(id=i, name=NAMES[i], rom=hex(MOVETYPES + 16 * i), icon_gfx=[hex(ic0), hex(ic1)],
                 can_use_fn=hex(can), execute_fn=hex(ex),
                 min_ep_to_start=a.get('min'), ep_cost_on_start=b.get('cost', a.get('min')))
        if i in STRINGS:
            d['ui_name'] = S(STRINGS[i][0]); d['ui_description'] = S(STRINGS[i][1])
        d.update(DETAILS.get(i, {}))
        out.append(d)
    return out

SPECIAL_MELEE = dict(
    names={c: S(0x9D + c) for c in range(5)}, descriptions={c: S(0xA2 + c) for c in range(5)},
    requires='record flags bit2 (script opcode 0x53 n=0)', player_state=0x27, attack_id=4, power=0x600, scale=1024,
    sequence='punch (A) -> keep holding: at punch end (16 f) state 0x27 (0x080085D8). Charge: anim to form.charge_anim_quads*4 frames then a 16-frame countdown (0x0800839A); '
             'release before countdown ends = cancel (no attack). Release after: entity+0x168=0 (invulnerable), lunge form.lunge px/f along facing (dir table 0x083B51E8), '
             'every 4th frame k with bit k in form.hit_quads_mask -> hit attack id 4 over rect 0x083B5208[facing]; ends at form.anim_len_quads*4 frames',
    rects_by_facing={['down', 'up', 'left', 'right'][f]: [s32(0x083B5208 + 16 * f + 4 * k) for k in range(4)] for f in range(4)},
    measured=dict(Goku=dict(hits=2, frames_after_release=[11, 15], total_frames=24), damage_examples={'Goku STR62': [244, 251]}))

STATES = {0x00: 'idle (input: dirs->walk/dash, A melee, B special, idle 1024 f -> 0x0E)', 0x01: 'walk', 0x0D: 'dash', 0x0E: 'idle animation',
          0x1F: 'recoil after dash collision (32 f, knockback)', 0x20: 'hurt (32 f, knockback 0.75 px/f, invulnerable)', 0x21: 'dying',
          0x22: 'energy blast', 0x23: 'transform', 0x24: 'revert / power-down (invulnerable)', 0x25: 'power-up (Piccolo)', 0x27: 'special melee charge/release',
          0x28: 'punch anim A', 0x29: 'punch anim B', 0x2A: 'punch anim C', 0x2B: 'energy punch anim', 0x2C: 'Scatter Shot', 0x2D: 'Masenko Ball',
          0x2E: 'Big Bang', 0x2F: 'Burning Attack', 0x30: 'Kamehameha', 0x31: 'Special Beam Cannon', 0x32: 'Sword Blast', 0x33: 'Spirit Bomb', 0x05: 'Hercule special'}

def build():
    return {
        'source': 'LoG2 ROM; generated by re/log2/extract_moves.py',
        'damage_formula': 'see formulas: dmg = ((((rand(26)+256) * (power*(1+crit)*(128-DEF)>>7) >> 8) * f(S)) >> 18) * scale >> 10, then * mult >> 7; f(S)=((26S-701)S+115618)S>>8; crit only melee, chance 164*STR/65536 (0x0800F910)',
        'scale_units': 'msg+4, 1024 = 1.0x',
        'attack_table_rom': hex(ATK), 'attack_entry_format': '8 bytes: u8 type (0 melee / 1 energy), u8 id, u16 pad, u32 power',
        'attack_table': attack_table(),
        'move_type_table_rom': hex(MOVETYPES),
        'move_type_entry_format': '16 bytes: ptr icon gfx (normal), ptr icon gfx (alt), canUse(fn)(&record.HP, player) -> bool, execute(fn)(&record.HP, player)',
        'player_moves': move_types(),
        'basic_melee': dict(attack_id=2, power=0x400, scale=1024, button='A', timing='hit resolves on the press frame (0x08007DE8); 16-frame punch anim (0x0800E19C)',
                            rects_by_facing={['down', 'up', 'left', 'right'][f]: [s32(0x083B5144 + 16 * f + 4 * k) for k in range(4)] for f in range(4)},
                            note='target hurtbox = actor descriptor rect, overlap test x0<hx1 && hx0<x1 && y0<hy1 && hy0<y1'),
        'special_melee': SPECIAL_MELEE,
        'player_states': {hex(k): v for k, v in sorted(STATES.items())},
        'player_attacks_doc': pl_attacks.__doc__,
        'player_attacks': pl_attacks.build(),
        'measurements_file': 're/log2/pl_measurements.json (raw per-run measurements; regenerate with pl_measure_all.py + pl_measure_compact.py)',
        'enemy_attacks': [],
    }

if __name__ == '__main__':
    d = build()
    os.makedirs(ROOT + 'data/log2', exist_ok=True)
    json.dump(d, open(ROOT + 'data/log2/moves.json', 'w'), indent=1)
    print('wrote data/log2/moves.json')
