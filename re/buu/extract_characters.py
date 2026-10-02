"""Generate data/buu/characters.json from the ROM (+ the emulator-measured form bonuses).

Sources (ROM addresses):
  character forms      0x086B82EC (54 x 24 B)   char_index -> record slot / growth row
  growth rows          0x086B72E0 (9 x 32 B)    used by LevelUp 0x08013678
  new-game records     0x086FDA4C (9 x 18 B)    applied by 0x0801339C
  skill table          0x0871DAF8               (names of the starting skills)
Form stat/speed bonuses come from ApplyRecordToPlayer variants (0x0800E1AE normal, 0x0800E2F2/0x0800E344 SS,
0x0800E470 SS3, 0x0800E550 speed/4, 0x0800E5C0 speed+64) and were confirmed for every form by
measure_forms.py (work/buu/forms_measured.json).
"""
import json
from common import *
from tables import *

CHAR_NAMES = ['Goku', 'Gohan', 'Goten', 'Trunks', 'Vegeta', 'Gotenks', 'Vegito', 'Gogeta', 'Other (Android 18/Hercule/Krillin/Piccolo/Pikkon/Tien/Videl/Yamcha)']
FLAG_BITS = {
    0x01: 'super_saiyan: energy costs + gauge use record+0x1E (SS gauge); drains 1 per 20 frames (0x0800E3E6)',
    0x02: 'super_saiyan_3: energy costs + gauge use record+0x20; drains (22*maxEP+0x3333)/65536 per frame (0x0800E4C4)',
    0x04: 'fusion: XP is split to both component characters; stats derived from components (0x0801375E)',
    0x08: 'fusion (timed/dance variant) [INFERRED]',
    0x10: 'can gain XP (GainExp 0x08013914 returns early without it)',
    0x20: 'fusion variant [INFERRED]',
    0x40: 'transformed form [INFERRED]',
}


def growth_doc(g):
    if not any(g):
        return None
    return dict(raw_u32=g,
                hp_add='(g0*rand(256)>>8) + g1   (16.16 fixed: HP += result>>16)',
                hp_pct='(g2*rand(256)>>8) + g3   (HP += HP*result>>16)',
                ep_add='g4*rand(256) + g5        (EP += result>>16)  [note: no >>8 on the random term]',
                ep_pct='(g6*rand(256)>>8) + g7   (EP += EP*result>>16)',
                hp_add_range=[g[1] / 65536, (g[0] * 255 / 256 + g[1]) / 65536],
                hp_pct_range=[g[3] / 65536, (g[2] * 255 / 256 + g[3]) / 65536],
                ep_add_range=[g[5] / 65536, (g[4] * 255 + g[5]) / 65536],
                ep_pct_range=[g[7] / 65536, (g[6] * 255 / 256 + g[7]) / 65536])


def main():
    measured = {}
    mp = WORK / 'forms_measured.json'
    if mp.exists():
        for d in json.loads(mp.read_text()):
            if d.get('ok'):
                measured[d['form']] = d
    forms = []
    for i in range(N_FORMS):
        f = form(i)
        f['flag_bits'] = [v for k, v in FLAG_BITS.items() if f['flags'] & k]
        m = measured.get(i)
        if m:
            f['measured_bonus'] = dict(STR=m['stat_bonus'][0], POW=m['stat_bonus'][1], END=m['stat_bonus'][2],
                                       END_energy=m['stat_bonus'][3], walk_speed=m['walk_bonus'], dash_speed=m['dash_bonus'],
                                       note='added to the player object on top of record stats; VERIFIED (emu)')
        forms.append(f)
    chars = []
    for ci in range(9):
        r = init_record(ci)
        r['skill_names'] = [skill(s)['name'] for s in r['skills']]
        r['xp'] = xp_for_level(r['level']) if 50 <= r['level'] <= 200 else 0
        chars.append(dict(index=ci, name=CHAR_NAMES[ci], record_ram=hex(0x03001560 + 0x68 * ci),
                          new_game=r,
                          growth=growth_doc(growth(ci)) if ci < 5 else dict(
                              raw_u32=growth(ci), note='unused: fusions (5-7) gain XP through their components and the '
                              'extra characters (8) have no can-gain-XP flag; row contents are filler'),
                          forms=[f['index'] for f in forms if f['char_index'] == ci]))
    out = dict(
        meta=dict(source='Dragon Ball Z: Buu\'s Fury (USA) ROM; see re/buu/notes.md', generator='re/buu/extract_characters.py',
                  units=dict(hp_ep='points', stats='points (u8 in record, u16 on player object)',
                             speed='1/256 pixel per frame (24.8 fixed point position units)')),
        level_range=dict(min=50, max=200, note='level = 50 + max{i: XP >= table[i]} (0x080135D2); see levels.json'),
        stat_points=dict(per_level=3, cost_per_stat_point=1, stats=['STR', 'POW', 'END', 'SPD'],
                         note='LevelUp 0x08013678 adds 3*(new-old) to record+0x2C; spent 1:1 on the Status screen (VERIFIED emu)'),
        level_up=dict(function='0x08013678', note='per level gained: 4 rand(256) draws -> HP/EP growth (see growth); HP and EP refilled to the new max. STR/POW/END/SPD do NOT grow automatically.'),
        movement=dict(walk='5*SPD + 102', dash='walk + 320', units='1/256 px per frame', function='0x0800E074',
                      dash_input='double-tap a direction within 20 frames (idle state 0x08013022)', status='VERIFIED (disasm + emu)'),
        fusion=dict(function='0x0801375E', maxHP_HP_maxEP_EP='(A+B)*154>>8', xp='(A.xp+B.xp)/2', STR_POW_END='(A+B)/2 + 10', SPD='(A+B)/2 + 16',
                    note='VERIFIED (disasm)'),
        characters=chars,
        forms=forms,
        form_flag_bits={hex(k): v for k, v in FLAG_BITS.items()},
        record_layout=RECORD_LAYOUT,
    )
    p = DATA / 'characters.json'
    p.write_text(json.dumps(out, indent=1))
    print('wrote', p)


RECORD_LAYOUT = {
    '+0x00': 'u8 ? (2 after new game; 3 for Goku in Snakeway state)',
    '+0x02': 'u16 fusion components: bits0-3 char A, bits4-7 char B, bits8-15 level',
    '+0x04': 'u32 SS cooldown/fatigue counter (decremented by (rec+0x62)>>6 per frame in normal form, 0x08013E06); SS needs it == 0',
    '+0x08': 's32 -1',
    '+0x0C': 'u8 number of known energy skills',
    '+0x0D..0x14': 'u8 skill ids (table 0x0871DAF8)',
    '+0x14': 'u8 current form (table 0x086B82EC)',
    '+0x15': 'u8 selected skill slot',
    '+0x16': 's16 HP', '+0x18': 's16 max HP', '+0x1A': 's16 EP', '+0x1C': 's16 max EP',
    '+0x1E': 'u16 Super Saiyan gauge', '+0x20': 'u16 Super Saiyan 3 gauge',
    '+0x24': 'u32 XP',
    '+0x28': 'u8 base STR', '+0x29': 'u8 base POW', '+0x2A': 'u8 base END', '+0x2B': 'u8 base SPD',
    '+0x2C': 'u16 unspent stat points',
    '+0x2E+2n': 'u8 skill level (0..2) for slot n, +0x2F+2n duplicate/max',
    '+0x3C/+0x40/+0x44/+0x48': 'u32 equipped item id: body / hands / feet / accessory (equipment table)',
    '+0x4C..+0x54': 'u16 effective STR, POW, END(melee def), END2(energy def), SPD (after equipment + Holy Water), RecalcStats 0x08013C66',
    '+0x56': 'u8 energy-attack cost factor /16 (16 = 100%; Dragon Armor halves)',
    '+0x57': 'u8 energy-block cost factor /16 (Mystic Aegis, Magician\'s Gloves halve)',
    '+0x58': 'u16 XP multiplier /256 (256 + armband/boot bonuses)',
    '+0x5A': 'u16 256 (unused/unknown multiplier)',
    '+0x5C': 'u16 thorns damage (Spiked Breastplate: n*0xA4>>6)',
    '+0x5E': 'u16 256 (unknown)',
    '+0x60': 'u16 block damage factor /65536 (0x1000 = 1/16; Jade Keiko halves)',
    '+0x62': 'u16 SS cooldown rate (0x4000; Super items x7/8)',
    '+0x64': 'u16 HP regen per frame /256 ("Z" Armor 5*0x445>>8 = 21)',
    '+0x66': 's8 crit bonus percent (Charge Gloves 5, Shock Boots 10, Soccer Cleats 20) -> obj+0x1BC = n*656',
    '+0x67': 'u8 copied from GameData+0x4F0 (player-object flag +0x1CC = (value == 0))',
}

if __name__ == '__main__':
    main()
