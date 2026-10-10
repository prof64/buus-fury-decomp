"""Playable character data -> data/log1/characters.json (LoG1 has one playable character: Goku).
Stats come from the level table (0x080114A8); movement/move constants from the code addresses cited."""
from romdata import *


def main():
    names = sprite_names()
    lv = level_table()
    sprites = {}
    for nm in ('goku', 'ssgoku', 'gokumallet'):
        i = names.index(nm)
        sprites[nm] = dict(sprite_id=i, **sprite_def(u8(SPRITE_TO_DEF + i)))
    goku = dict(
        name='Goku', team=1, char_struct='0x02013224 (first 0x278-byte char slot)', controller='0x030007AC',
        base_stats=dict(level=1, max_hp=lv[0]['max_hp'], max_ki=lv[0]['max_ki'],
                        max_flight_energy=lv[0]['max_flight_energy'], defense=250, exp=0),
        defense=dict(value=250, constant=True, source='init_char 0x08006E62 (movs r0,#0xFA; str r0,[r4,#0x60])'),
        growth='per-level absolute values from levels.json (no formula; table lookup)',
        growth_table=[dict(level=r['level'], exp=r['exp_required'], max_hp=r['max_hp'], max_ki=r['max_ki'],
                           max_flight_energy=r['max_flight_energy']) for r in lv],
        max_level=len(lv),
        regen=dict(ki='+(maxKi >> 6) every frame where frame_counter & 15 == 0, not while flying (0x08008018)',
                   hp='none (only items / scripts)', flight_energy='none (feather pickups only)'),
        movement=dict(walk_px_per_frame=1.0, walk_raw='controller+6 = 16 (1/16 px units) x multiplier 1 (0x08002CE0)',
                      fly_px_per_frame=2.0, fly_raw='multiplier 2 in flight state 14 (0x08002AC6)',
                      diagonal='NO diagonal movement: one direction per frame, priority UP > DOWN > LEFT > RIGHT '
                               '(if/else chain 0x08002CE0..0x08002DFA; VERIFIED test_diag.py)',
                      position_units='1/16 px at controller+0x14/+0x16'),
        collision_footprint=dict(w=16, h=8, note='feet box from entity position'),
        hurtbox=sprites['goku']['hurtbox'],
        invulnerability_after_hit='none for the player (state-6 handler only flashes and resets the player to Standing)',
        damage_taken_cap='maxHP // 3 + 1 per enemy hit (0x03000C1C; set by 0x08006D70 on every stat refresh)',
        moves=['melee', 'Ki Blast (id 0, always)', 'Kame Hame Ha (id 1, learned flag bit1 @0x0300004D)',
               'Solar Flare (id 2, learned flag bit2 @0x0300004D)', 'Flight (R)'],
        controls=dict(A='punch / talk / pick up (msg 0 broadcast)', B='fire/charge selected ki move',
                      L='cycle selected ki move among learned ones (0x08003524)', R='take off / land (flight)',
                      START='pause menu', SELECT='read only by an overlay entity (0x08003DBE); no combat effect found'),
        cheats=dict(invincibility=dict(input='UP, DOWN, LEFT, RIGHT, B, A during the Shenron intro scene',
                                       code='0x080048FA (sequence u16[6] at 0x08011462)', effect='0x03000C18 = 1: player HP never decreases',
                                       verified='VERIFIED (test_cheat.py)')),
        sprites=sprites,
        notes=['Super Saiyan Goku (ssgoku) and the mallet Goku (gokumallet, King Kai/Gregory minigame) are sprite '
               'variants selected by scripts (controller+4); no separate stat table was found -> INFERRED same stats.'])
    dump('characters.json', dict(source='LoG1 (USA); re/log1/extract_characters.py', characters=[goku]))


if __name__ == '__main__':
    main()
