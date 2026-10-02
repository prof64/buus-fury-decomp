"""Regenerate data/log2/characters.json from the LoG2 ROM.

Static tables read from ROM (addresses cited); behavioural constants that live in code are
embedded with the ROM address of the instruction/literal they came from (see notes.md Part B).
"""
import json, struct, os
from common import ROM, u8, u16, u32, s32, ROOT

def cstr(a):
    o = a & 0x1ffffff
    return ROM[o:ROM.index(b'\0', o)].decode('latin1')

STRTAB = 0x080E2840          # string pointer table used by UI code (index*4)
def S(i): return cstr(u32(STRTAB + 4 * i))

INIT = 0x081D9548            # 6 x 28-byte initial party records (DMA'd to 0x03000E9C by 0x08003C24)
GROWTH = 0x081D95F0          # 6 x 12 bytes: (min,max) u16 per stat per level, units 1/256 stat point
EXPTAB = 0x081D9834          # 50 x u32 cumulative exp; level = 1 + max i with exp >= tab[i]
FORMS = 0x081D967C           # 22 x 20-byte form records, indexed by record+8
MOVETYPES = 0x081D9900       # 14 x 16-byte special move types (see extract_moves.py)
ACTOR_PTRS = 0x083B4E74      # actor/graphics descriptor pointer table (id*4), via 0x08009324
NAMES = ['Gohan', 'Piccolo', 'Vegeta', 'Trunks', 'Goku', 'Hercule']   # strings 0x080E2DF4..
PARTY_RAM = 0x03000E90       # u8 active index; records at 0x03000E9C + 28*i

# Form controller singletons (IWRAM) are initialised at 0x080232E4 with these vtables.
# Behaviour decoded from their methods (slot0 = apply stats/speeds, slot1 = per-frame tick):
CONTROLLERS = {
    0x030021E0: dict(vtable=0x08024004, name='normal', apply_fn=0x08016FD8, tick_fn=0x08017028,
        stat_bonus='STR,POW,END = record>>8 (no bonus)', stat_override=None, bonus=[0, 0, 0],
        walk_speed=1.25, run_speed=2.5,  # floats 0x3FA00000/0x40200000 @0x0801700A/0x08017018 -> *256 fixed (0x080265C4)
        tick='acc+=110/frame (0x0801703E); on 16-bit overflow (~every 595.8 f): active EP += maxEP*0x7AE>>16 (3.0%), '
             'every OTHER party member HP += maxHP*0xCCD>>16 and EP += maxEP*0xCCD>>16 (5.0%) (0x08017028-0x080170CA)',
        ep_drain=None, hp_regen=None),
    0x030021E4: dict(vtable=0x08024E4C, name='full-power (Gohan SSJ2, form 4)', apply_fn=0x080170D8, tick_fn=0x0801710E,
        stat_override=[100, 100, 100], bonus=None, walk_speed=2.0, run_speed=3.25,
        tick='acc+=0xCCD/frame (every 20 f): active HP += maxHP*0x290>>16 (1.0%); no EP drain',
        ep_drain=None, hp_regen={'period_frames': 20, 'fraction_of_max': 0x290 / 65536}),
    0x030021E8: dict(vtable=0x0802526C, name='bulky super saiyan (slow; Vegeta form 10, Trunks form 14)', apply_fn=0x0801718C,
        tick_fn=0x080171E4, stat_override=None, bonus=[15, 10, 15], walk_speed=0.75, run_speed=1.75,
        tick='acc+=0x667/frame (every ~40 f): EP -= 1; when EP reaches 0 -> state 0x24 (revert)',
        ep_drain={'period_frames': 40, 'amount': 1}, hp_regen=None),
    0x030021EC: dict(vtable=0x08024C58, name='super saiyan (forms 5,11,16,17,20)', apply_fn=0x080172B0, tick_fn=0x08017306,
        stat_override=None, bonus=[10, 10, 10], walk_speed=1.75, run_speed=3.0,
        tick='acc+=0xCCD/frame (every 20 f): EP -= 1; at 0 -> state 0x24 (revert)',
        ep_drain={'period_frames': 20, 'amount': 1}, hp_regen=None),
    0x030021F0: dict(vtable=0x08024ABC, name='Piccolo powered-up (no cape, form 7)', apply_fn=0x080173AA, tick_fn=0x08017400,
        stat_override=None, bonus=[6, 6, 6], walk_speed=1.75, run_speed=3.0,
        tick='every 20 f: HP += maxHP*0x290>>16 (1%), EP -= 1; at EP 0 -> revert',
        ep_drain={'period_frames': 20, 'amount': 1}, hp_regen={'period_frames': 20, 'fraction_of_max': 0x290 / 65536}),
}
FORM_LABELS = {  # from in-emulator screenshots of each form (work/log2/pl_forms.png, re/log2/pl_formshots.py)
    0: 'Gohan (dark gi; debug default, no transform target)', 1: 'Gohan (white Saiyan armor)',
    2: 'Gohan (blue Saiyan armor)', 3: 'Gohan (initial outfit)', 4: 'Super Saiyan 2 Gohan (scripted)',
    5: 'Super Saiyan Gohan', 6: 'Piccolo (turban & cape)', 7: 'Piccolo (cape off, powered up)',
    8: 'Vegeta', 9: 'Vegeta (alt armor)', 10: 'Super Vegeta (bulky SSJ)', 11: 'Super Saiyan Vegeta',
    12: 'Trunks (jacket)', 13: 'Trunks (long hair, armor)', 14: 'Super Trunks (bulky SSJ)',
    15: 'Trunks (long hair, armor, alt)', 16: 'Super Saiyan Trunks (armor)', 17: 'Super Saiyan Trunks (jacket)',
    18: 'Future Trunks (prologue)', 19: 'Goku', 20: 'Super Saiyan Goku', 21: 'Hercule'}

def actor_hurtboxes(aid):
    if aid < 7: return None
    p = u32(ACTOR_PTRS + 4 * aid)
    out = {}
    for d, dn in enumerate(['down', 'up', 'left', 'right']):
        out[dn] = [s32(p + 0x0C + 16 * d + 4 * k) for k in range(4)]
    return {'descriptor': hex(p), 'hurtbox_by_facing_x0y0x1y1': out}

def forms():
    out = []
    for i in range(22):
        a = FORMS + 20 * i
        flags = u8(a)
        ctl = u32(a + 0x10)
        c = CONTROLLERS[ctl]
        out.append(dict(
            id=i, label=FORM_LABELS[i], rom=hex(a),
            flags=flags, transformed=bool(flags & 1), cannot_use_transform_slot=bool(flags & 2),
            actor_id=u8(a + 1), aux_gfx_ids=[u8(a + 2), u8(a + 3)],
            transform_to_form=u8(a + 4), revert_to_form=u8(a + 5),
            special_melee=dict(anim_len_quads=u8(a + 6), attack_frames=u8(a + 6) * 4,
                               charge_anim_quads=u8(a + 7), hit_quads_mask=u8(a + 8),
                               hit_on_anim_frames=[b for b in range(8) if u8(a + 8) >> b & 1],
                               lunge_speed_px_per_frame=u32(a + 0x0C) / 256.0,
                               raw_lunge=u32(a + 0x0C)),
            controller=dict(ram=hex(ctl), vtable=hex(c['vtable']), name=c['name']),
            hurtbox=actor_hurtboxes(u8(a + 1)),
        ))
    return out

def move_type_rows():
    rows = []
    for i in range(14):
        rows.append(struct.unpack_from('<4I', ROM, (MOVETYPES & 0x1ffffff) + 16 * i))
    return rows

def chars():
    out = []
    exptab = [u32(EXPTAB + 4 * i) for i in range(50)]
    for c in range(6):
        a = INIT + 28 * c
        rec = dict(hp=u16(a), max_hp=u16(a + 2), ep=u16(a + 4), max_ep=u16(a + 6), form=u8(a + 8),
                   selected_slot=u8(a + 9), STR_8p8=u16(a + 0xA), POW_8p8=u16(a + 0xC), END_8p8=u16(a + 0xE),
                   STR=u16(a + 0xA) / 256, POW=u16(a + 0xC) / 256, END=u16(a + 0xE) / 256,
                   exp=u32(a + 0x10), flags=u32(a + 0x14), slots=[u8(a + 0x18 + k) for k in range(4)])
        lvl = max(i for i in range(50) if rec['exp'] >= exptab[i]) + 1
        g = GROWTH + 12 * c
        growth = {}
        for k, st in enumerate(['STR', 'POW', 'END']):
            mn, mx = u16(g + 4 * k), u16(g + 4 * k + 2)
            growth[st] = dict(min_8p8=mn, max_8p8=mx, min=mn / 256, max=mx / 256, avg=(mn + mx) / 512)
        slots = []
        for k, mid in enumerate(rec['slots']):
            if c == 5:
                name, desc = 'Hercule special', None
            elif k == 0:
                name, desc = S(0xA7 + c), S(0xAC + c)
            elif k == 1:
                name, desc = S(0xB1 + c), S(0xB6 + c)
            elif k == 2:
                name, desc = ('Super Saiyan transformation' if mid == 12 else 'Power-up (Piccolo, move type 11)'), None
            else:
                name, desc = 'Energy blast (basic ki blast)', None
            slots.append(dict(slot=k, move_type=mid, name=name, description=desc,
                              unlock_flag_bit=3 + k, unlocked_at_start=bool(rec['flags'] >> (3 + k) & 1)))
        sm = None if c == 5 else dict(name=S(0x9D + c), description=S(0xA2 + c), unlock_flag_bit=2,
                                      unlocked_at_start=bool(rec['flags'] & 4))
        out.append(dict(index=c, name=NAMES[c],
                        init_record_rom=hex(a), initial=rec, initial_level=lvl,
                        growth_per_level=growth, growth_rom=hex(g),
                        move_slots=slots, special_melee=sm,
                        forms=[f for f in range(22) if [0,0,0,0,0,0,1,1,2,2,2,2,3,3,3,3,3,3,3,4,4,5][f] == c]))
    return out

def build():
    exptab = [u32(EXPTAB + 4 * i) for i in range(50)]
    return {
        'source': 'Dragon Ball Z: The Legacy of Goku II (USA) ROM; generated by re/log2/extract_characters.py',
        'units': {'position': 'pixels; entity coords are 24.8 fixed (entity+0x158/+0x15C)',
                  'speed': 'pixels per frame (60 fps)', 'stat_8p8': 'stat * 256 as stored in RAM record',
                  'frames': '1/60 s'},
        'party_record_layout': {
            'ram': '0x03000E9C + 28*index (u8 active index at 0x03000E90)',
            'fields': {'+0x00 u16': 'HP', '+0x02 u16': 'max HP', '+0x04 u16': 'EP', '+0x06 u16': 'max EP',
                       '+0x08 u8': 'form id (index into form table 0x081D967C)',
                       '+0x09 u8': 'selected special slot 0..3 (cycled with L: 0x08004284)',
                       '+0x0A u16': 'STR (8.8, cap 0x6400=100.0)', '+0x0C u16': 'POW (8.8)', '+0x0E u16': 'END (8.8)',
                       '+0x10 u32': 'experience', '+0x14 u32': 'flags (bit0 in party/selectable, bit1 ?, bit2 special melee learned, bits3..6 slot 0..3 learned)',
                       '+0x18 u8[4]': 'move-type id per slot (table 0x081D9900)'}},
        'max_level': 50, 'exp_table_rom': hex(EXPTAB), 'exp_table': exptab,
        'level_formula': 'level = 1 + max{i : exp >= exp_table[i]}  (0x08004214)',
        'level_up': {
            'function': '0x08003CA0 (called from exp award 0x08003D58 when level changes)',
            'stats': 'for each of STR,POW,END: stat += min*n + rand((max-min+1)*n); cap 0x6400 (0x08003C6E); n = levels gained',
            'hp': 'per level: maxHP += maxHP*(rand(0x51F)+0xE14)>>16  (5.50%..7.50% compounding); HP = maxHP',
            'ep': 'per level: maxEP += rand(3)+2  (2..4); EP = maxEP',
            'rand': 'rand(n) = ((rng()&0xFFFF)*n)>>16, rng at 0x080213FC (see formula.py)'},
        'exp_award': {'function': '0x0800E4D2', 'formula': 'E = enemy_class.exp; if E>5 and level<50: span = exp_table[L]-exp_table[L-1]; E = max(E, span>>7); E = min(E, span>>1)  (only the active character gains)'},
        'regen': CONTROLLERS[0x030021E0]['tick'],
        'form_controllers': {hex(k): {kk: (hex(vv) if isinstance(vv, int) and kk.endswith(('vtable', '_fn')) else vv)
                                      for kk, vv in v.items()} for k, v in CONTROLLERS.items()},
        'movement': {
            'walk_speed': 'entity+0x188 (24.8), set by form controller: 1.25 px/f normal (VERIFIED 1.25 px/f)',
            'diagonal': 'both axes at full walk speed (no normalisation) - VERIFIED',
            'dash': 'double-tap a direction within 20 frames (0x08007FC6) -> state 0x0D: moves at run speed entity+0x18C (2.5 px/f normal, VERIFIED) while held; perpendicular steering at walk speed; blocked -> state 0x1F recoil (32 f, 0.75 px/f back)',
            'facing': '0=down 1=up 2=left 3=right (entity+0x10)'},
        'combat_common': {
            'melee': 'A: instant hit on press (0x08007DE8): attack id 2 (power 0x400) scale 0x400, rect = pos + table 0x083B5144[facing]; then random punch anim state 0x28/0x29/0x2A for 16 frames',
            'melee_rect_by_facing_x0y0x1y1': {['down', 'up', 'left', 'right'][f]: [s32(0x083B5144 + 16 * f + 4 * k) for k in range(4)] for f in range(4)},
            'hurt': 'state 0x20: 32 frames, pushed 0.75 px/f opposite to own facing (table 0x083B51C8), damage multipliers set 0 (invulnerable) - VERIFIED',
            'death': 'HP 0 -> state 0x21, game-over object after 200 frames (0x080087C6)',
            'special_melee': 'hold A after a punch with flag bit2: state 0x27 charge; needs anim charge + 16 frames; release -> lunge + hits (attack id 4, 0x600) on frames from form.hit_on_anim_frames, rect table 0x083B5208; invulnerable during release (entity+0x168=0)'},
        'characters': chars(),
        'forms': forms(),
        'forms_rom': hex(FORMS),
    }

if __name__ == '__main__':
    d = build()
    os.makedirs(ROOT + 'data/log2', exist_ok=True)
    json.dump(d, open(ROOT + 'data/log2/characters.json', 'w'), indent=1)
    print('wrote data/log2/characters.json')
