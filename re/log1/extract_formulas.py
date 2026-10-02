"""Machine-readable formulas -> data/log1/formulas.json and data/log1/rng.json.
Expressions use C-like integer semantics: '//' = truncating signed division (ARM div routine 0x0800E568
rounds toward zero), '>>' arithmetic shift, all values 32-bit signed unless noted.
Each entry cites the code address and verification status."""
from romdata import *


def main():
    speeds = [speed_class(i) for i in range(8)]
    formulas = dict(
        source='LoG1 (USA); re/log1/extract_formulas.py; reference implementation in re/log1/model.py',
        conventions=dict(division='truncating toward zero', rand='rand(n) main RNG -> integer in [0, n] (inclusive!)',
                         rand2='rand2(n) secondary RNG -> integer in [0, n-1]', frame='60 Hz frame counter 0x03003B6C'),
        damage_pipeline=dict(
            order=['attacker computes raw damage (per attack type below)',
                   'hit() 0x08007416: if defender invulnerable (char+0x14 > 0) -> "clink" sfx 0x1D, no damage',
                   'if attacker is an enemy: raw = min(raw, cap) where cap = player_maxHP // 3 + 1 (0x03000C1C)',
                   'if attacker is the player: defender aggro timer char+0x1E = 900 + rand(900) frames',
                   'apply_damage() 0x08007352 (defense roll), see below',
                   'result 2 (damaged): defender -> hurt state 6; enemies get stun 20 frames (char+0x0A) and '
                   'invulnerability 80 frames (char+0x14); player only flashes and is reset to Standing',
                   'result 1 (killed): defender state 7; if killer is the player: exp += victim exp reward, level up check',
                   'result 0 (no damage): clink sfx 0x1D (rate-limited 20 frames)'],
            verified='VERIFIED 113/113 apply_damage outcomes (verify_damage.py)'),
        apply_damage=dict(
            code='0x08007352', inputs=['dmg', 'def = defender char+0x60', 'hp = defender char+0x4C'],
            early_out='if defender is not the player: return 0 when defender char+0x0A (stun) != 0, '
                      'or when char+0x0B == 2 (dormant) and def != 0',
            defense_roll='if def != 0: r = rand(10000); '
                         'r > 9000: dmg = dmg - def (0 if def > dmg); '
                         '7500 < r <= 9000: dmg = dmg - def//2 (0 if def//2 > dmg); '
                         '5000 < r <= 7500: dmg = dmg - def//4 (0 if def//4 > dmg); '
                         '4800 < r <= 5000: return 0 (miss); '
                         'r <= 4800: full dmg',
            probabilities=dict(full=4801 / 10001, miss=200 / 10001, quarter_def=2500 / 10001, half_def=1500 / 10001,
                               full_def=1000 / 10001),
            kill='if dmg >= hp: hp = 0, alive (char+0x9C) = 0, return 1 (unless god-mode flag 0x03000C18 and defender is player)',
            damage='elif dmg != 0: hp -= dmg, return 2; else return 0',
            note='signed compares: a negative dmg (see enemy melee bug) heals the defender when the roll is <= 4800'),
        player_melee=dict(expr='(maxHP * (384 + rand2(128))) >> 9', code='0x08005676', verified='VERIFIED 10/10'),
        player_ki_blast=dict(expr='maxHP // 3', code='0x0800B596', verified='VERIFIED'),
        player_kamehameha=dict(expr='ki_spent_charging', code='0x0800BD20 (msg damage = projectile+0x24)', verified='VERIFIED'),
        charge_cost_player=dict(expr='(maxKi + 15) >> 4', period_frames=8, code='0x0800B50A / 0x0800B436'),
        solar_flare_duration=dict(expr='60 + min(charge, maxKi) * 300 // maxKi', cooldown='duration + 60',
                                  code='0x0800B692', verified='VERIFIED (charge 125/400 -> 153 frames)'),
        enemy_melee=dict(expr='hp // 4 + rand(hp // 2)', hp='attacker CURRENT hp (char+0x4C)', code='0x0800F7EC',
                         bug='rand(n) uses signed asr: when (prod*(n+1)) >= 2^31 (possible once n >= 32768) the '
                             'result is negative -> damage can be negative (heals the player on a full-damage roll)',
                         verified='VERIFIED range + negative case observed'),
        enemy_ki_blast=dict(expr='maxHP // 3', power_expr='rand(ki // 2) + ki // 4', chance='rand(100) <= 1',
                            code='0x0800F8FC / 0x0800B596', verified='VERIFIED'),
        damage_cap_vs_player=dict(expr='player_maxHP // 3 + 1', code='0x08006D70'),
        exp=dict(on_kill='player_exp += victim.exp_reward (template +0x0C)', code='0x08007518',
                 level='largest i with level_table[i].exp <= exp (level = i + 1)', code_level='0x08005C32',
                 table='levels.json', verified='VERIFIED (340 + wolf 70 -> level 2, HP 80 -> 175, maxKi 150)'),
        level_up=dict(code='0x08005BFE', maxHP='table', maxKi='table', hp='hp += newMaxHP - oldMaxHP',
                      ki='unchanged', ki_regen='maxKi >> 6', defense='unchanged (250)'),
        ki_regen=dict(expr='ki = min(maxKi, ki + (maxKi >> 6))', when='frame_counter & 15 == 0 and not flying (0x030007DC == 0) [player]; '
                      'enemies: frame_counter & 15 == 0 while awake', code='0x08008032 / 0x08008312',
                      verified='VERIFIED (+1 per 16 frames at maxKi 100)'),
        hp_regen=dict(expr='none', verified='VERIFIED (no change over 960 frames)'),
        heal_items=dict(herb='hp = min(maxHP, hp + maxHP // 4)', senzu='hp = maxHP', code='0x0800170C / 0x08001730'),
        flight=dict(speed='2 px/frame', cost='1 energy per 8 frames', max='level table col 3', code='0x08002A88'),
        movement=dict(player_walk='16/16 px per frame', enemy='SPEED_TABLE[speed_class] / 16 px per frame',
                      speed_table=dict(addr=h(SPEED_TABLE), values_sixteenths=speeds)),
        enemy_wake=dict(expr='dist2_px(player, enemy) < (96 * aggro_tiles^2) & 0xFFFF', code='0x080085D2 / 0x08008176',
                        also='or aggro timer char+0x1E != 0 (set 900..1800 frames when the player hits it); the enemy '
                             'must also be within 144 px (0x5100 = 144^2) of the screen centre'),
        enemy_target=dict(expr='manhattan_tiles(enemy, player) <= aggro_tiles', code='0x08007680'),
        enemy_melee_range=dict(expr='manhattan_tiles <= 1', code='0x0800F782'),
        timers=dict(enemy_attack_cooldown=45, enemy_hurt_stun=20, enemy_invuln_after_hit=80, punch_anim=16,
                    clink_sfx_ratelimit=20, aggro_after_being_hit='900 + rand(900)', kame_max_growth_frames=16),
    )
    lv = level_table()
    formulas['lookup_tables'] = dict(
        level_exp_required=[r['exp_required'] for r in lv],
        level_max_hp=[r['max_hp'] for r in lv],
        level_max_ki=[r['max_ki'] for r in lv],
        level_max_flight_energy=[r['max_flight_energy'] for r in lv],
        level_hp_gain=[0] + [lv[i]['max_hp'] - lv[i - 1]['max_hp'] for i in range(1, len(lv))],
        level_ki_gain=[0] + [lv[i]['max_ki'] - lv[i - 1]['max_ki'] for i in range(1, len(lv))],
        enemy_speed_sixteenths_by_class=speeds,
        note='index 0 = level 1; source table 0x080114A8 (see levels.json)')
    dump('formulas.json', formulas)
    rng = dict(
        main=dict(code='0x08005CA8', state=dict(a='0x0300098C', b='0x03000990'), seed='zero at boot (never seeded); '
                  'advances only when gameplay code calls it',
                  step=['a = (a + 0x02CBBC71) ^ b', 'b = (rotl32(b, 3) + 0x0632D80F) ^ a  (uses NEW a)'],
                  output='rand(n) = asr32(int32(((a*b) & 0xFFFF) * (n+1)), 16)  -> [0, n]; negative if product >= 2^31',
                  python='re/log1/model.py Rng'),
        secondary=dict(code='0x0800A6F4 (raw), 0x0800A714 rand2(n), 0x0800A724 chance(p)', state=dict(a='0x03003D24', b='0x03003D28'),
                       step='same recurrence as main', raw_output='a*b (32-bit)',
                       rand2='((raw >> 16) * n) >> 16 -> [0, n-1]', chance='(raw >> 16) < p',
                       advances='at least once per frame (0x0800A724 is called every frame), so it depends on timing'),
        uses=dict(main=['defense roll rand(10000)', 'enemy melee/ki rand', 'aggro timer rand(900)',
                        'AI path timer rand(60)+30', 'AI wander, flee 6% roll, ki-blast 2% roll, dormant wake rand(65535)<200'],
                  secondary=['player punch damage rand2(128)', 'misc visual/sfx rolls']))
    dump('rng.json', rng)


if __name__ == '__main__':
    main()
