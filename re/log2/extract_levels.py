"""Extract LoG2 exp table, initial character records, per-level stat growth -> data/log2/levels.json"""
import json, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from common import *

EXP_TABLE = 0x081d9834
INIT_RECORDS = 0x081d9548
GROWTH = 0x081d95f0
NAMES = ['Gohan', 'Piccolo', 'Vegeta', 'Trunks', 'Goku', 'Hercule']

def main():
    exp = [u32(EXP_TABLE + 4 * i) for i in range(50)]
    levels = []
    for L in range(1, 51):
        lo = exp[L - 1]; hi = exp[L] if L < 50 else None
        span = (hi - lo) if hi is not None else None
        levels.append(dict(level=L, total_exp=lo, exp_to_next=span,
                           award_min=(span >> 7) if span else None, award_max=(span >> 1) if span else None))
    chars = []
    for i, n in enumerate(NAMES):
        o = INIT_RECORDS + 28 * i
        hp, mhp, ep, mep = (u16(o + 2 * k) for k in range(4))
        g = [u16(GROWTH + 12 * i + 2 * k) for k in range(6)]
        chars.append(dict(
            index=i, name=n, record_rom=hex(o),
            initial=dict(hp=hp, max_hp=mhp, ep=ep, max_ep=mep, form_id=u8(o + 8), selected_slot=u8(o + 9),
                         STR_raw=u16(o + 0xa), POW_raw=u16(o + 0xc), END_raw=u16(o + 0xe),
                         STR=u16(o + 0xa) / 256, POW=u16(o + 0xc) / 256, END=u16(o + 0xe) / 256,
                         exp=u32(o + 0x10), flags=hex(u32(o + 0x14)), move_slots=list(ROM[(o + 0x18) & 0x1ffffff:(o + 0x1c) & 0x1ffffff])),
            growth_rom=hex(GROWTH + 12 * i),
            growth_per_level_raw=dict(STR=[g[0], g[1]], POW=[g[2], g[3]], END=[g[4], g[5]]),
            growth_per_level=dict(STR=[g[0] / 256, g[1] / 256], POW=[g[2] / 256, g[3] / 256], END=[g[4] / 256, g[5] / 256]),
            growth_avg_per_level=dict(STR=(g[0] + g[1]) / 512, POW=(g[2] + g[3]) / 512, END=(g[4] + g[5]) / 512)))
    out = dict(
        source='LoG2 ROM; extract_levels.py',
        max_level=50,
        exp_table=dict(addr=hex(EXP_TABLE), desc='u32 x50: total exp required for level L is exp_table[L-1]; level = 1 + max i with exp >= table[i] (0x08004214)'),
        levels=levels,
        record_layout=dict(base='party 0x03000e90; record i at 0x03000e9c + 28*i (i: 0 Gohan,1 Piccolo,2 Vegeta,3 Trunks,4 Goku,5 Hercule); active index = byte at 0x03000e90',
                           fields={'+0 s16': 'HP', '+2 s16': 'maxHP', '+4 s16': 'EP', '+6 s16': 'maxEP', '+8 u8': 'form id (row of 0x081d967c; player agent)',
                                   '+9 u8': 'selected special slot', '+0xa u16': 'STR 8.8 fixed', '+0xc u16': 'POW 8.8', '+0xe u16': 'END 8.8',
                                   '+0x10 u32': 'exp', '+0x14 u32': 'flags: bit0 = character unlocked/selectable (Switch list, status list); bit1 = counted by 0x08003e1e (INFERRED in-party); bits3..6 = special slots 0..3 usable',
                                   '+0x18 u8[4]': 'move-type ids per slot (table 0x081d9900)'}),
        characters=chars,
        level_up=dict(
            function='0x08003ca0(party, idx, oldLevel, newLevel)', verified=True,
            stat_growth='for each of STR,POW,END: stat += min*n + rand((max-min+1)*n), n = newLevel-oldLevel; stat = min(stat, 0x6400 (100.0)). One RNG call per stat (in STR,POW,END order) regardless of n.',
            hp_growth='per level gained (loop old..new-1): maxHP += (maxHP * (rand(0x51f) + 0xe14)) >> 16  [x1.0550 .. x1.0750]; HP = maxHP',
            ep_growth='per level gained (same loop, after HP roll): maxEP += rand(3) + 2  [+2..+4]; EP = maxEP',
            rng_order='rand(STR) rand(POW) rand(END) then per level: rand(0x51f) rand(3)',
            message='script 0x083b6524 "%s has achieved Level %d!" + "Attribute Bonus" with the integer stat deltas stored at 0x03001fc1..3',
            note='non-active characters never gain exp from kills; story scripts can force a level with script op 0x0800ac0x (set level of char to N if lower: runs 0x08003ca0 then sets exp=table[N-1]); debug Map Test sets every character to level 40 the same way'),
        exp_award=dict(
            function='0x0800e4d2 (enemy death)', verified=True,
            rule='E = enemyClass.exp (row+0). If E==0: nothing. If E>5 and active level < 50: span = table[L]-table[L-1]; '
                 'if (span>>7) > E: E = span>>7; elif (span>>1) < E: E = span>>1. (E<=5 is awarded raw.) Level-50 characters get nothing. '
                 'The amount is queued as a message object (vtable 0x080245b0) and applied to the ACTIVE character only via 0x08003d58.',
            example='Piccolo L40 killing a Snake (class 53, exp 13): span=607735, min=607735>>7=4747 -> +4747 exp (VERIFIED in emulator)'),
        regen=dict(
            mechanism='Player "power mode" objects (5 handlers, pointer table built at 0x080232e4 into 0x030021e0; the active one is player+0x180, '
                      'chosen by the last field of the form table 0x081d967c). Each mode accumulates acc(player+0x184) += step every frame '
                      '(independent of input/movement) and fires when acc crosses 0x10000 (acc &= 0xffff).',
            modes=[
                dict(mode=0, handler='0x08017029', step=0x6e, period_frames=65536 / 0x6e,
                     effect='active char: EP += maxEP*0x7ae>>16 (3.0%); every OTHER character (all 6 records except active): HP += maxHP*0xccd>>16 and EP += maxEP*0xccd>>16 (5%), clamped',
                     stats='entity STR/POW/END = record>>8, dmg-taken mults 128/128', verified=True,
                     measured='Piccolo maxEP 137: EP 10->14->18 at 596-frame intervals (frames 2739, 3335)'),
                dict(mode=1, handler='0x0801710f', step=0xccd, period_frames=20.0,
                     effect='active HP += maxHP*0x290>>16 (1.0%) every 20 frames; no EP change', stats='STR=POW=END=100 fixed'),
                dict(mode=2, handler='0x080171e5', step=0x667, period_frames=65536 / 0x667,
                     effect='EP -= 1 every ~40 frames; at EP 0 (and HP>0) reverts (player state 0x24)', stats='STR+15, POW+10, END+15'),
                dict(mode=3, handler='0x08017307', step=0xccd, period_frames=20.0,
                     effect='EP -= 1 every 20 frames; reverts at EP 0', stats='STR+10, POW+10, END+10'),
                dict(mode=4, handler='0x08017401', step=0xccd, period_frames=20.0,
                     effect='HP += maxHP*0x290>>16 (1%) and EP -= 1 every 20 frames; reverts at EP 0', stats='STR/POW/END from record'),
            ],
            note='No natural HP regen for the active character in normal mode. Transform semantics (which form uses which mode) belong to the player-moves notes; '
                 'modes 1-4 are INFERRED from disassembly only. Script ops 0xe9-0xec (0x0800b0a1..) set active HP/maxHP/EP/maxEP and 0xb7/0xb8 set any char HP/EP '
                 '(used by story scripts, e.g. healing events).'),
    )
    path = os.path.join(ROOT, 'data/log2/levels.json')
    json.dump(out, open(path, 'w'), indent=1)
    print('wrote', path)

if __name__ == '__main__':
    main()
