"""Script lockstep scenarios: micro scenarios (one native / command class each, bytecode through the game's runner)
and full cutscenes (event graphs). `python scenarios.py [micro|full|all|<name>...]` records them into
data/script_lockstep/ (micro/<name>.json, <name>.json). The replay is engine/crates/wf-script-core/tests/lockstep.rs.

Micro scenarios start from buu_snakeway_start.ss: Goku (script sprite 117, actor 0) at (144, 152) facing left,
map NPCs 135 (woman, (66, 210)), 40 and 161."""
import sys
from capture import asm, record_code, record_event, save, mash, KEY, OUT

SW = 'buu_snakeway_start.ss'
N = lambda n: ('n', n)  # noqa: E731


def keys_at(*frames, key=KEY.A, hold=1):
    s = set()
    for f in frames:
        s.update(range(f, f + hold))
    return lambda f: key if f in s else 0


# name -> (items, frames, keys)
MICRO = {
    # actor commands on the player
    'walk_right': ([0, 184, 152, 200, N(0x44)], 80, None),
    'walk_diag': ([0, 184, 182, 150, N(0x44)], 120, None),
    'walk_up': ([0, 140, 120, 100, N(0x44)], 120, None),
    'walk_wait': ([0, 184, 152, 200, N(0x44), 0, 3, N(0x4A)], 80, None),
    'move_to': ([0, 184, 152, 200, N(0x50)], 80, None),
    'walk60': ([0, 184, 170, N(0x60)], 120, None),
    'walk61': ([0, 120, 140, 150, N(0x61)], 120, None),
    'walk5f': ([0, 135, 16, 0, N(0x5F)], 200, None),
    'glide': ([0, 184, 170, 150, N(0x49)], 120, None),
    'jump': ([0, 184, 170, 20, N(0x45)], 80, None),
    'set_pos': ([0, 184, 170, N(0x5D)], 20, None),
    'face': ([0, 1, N(0x53), 0, 3, N(0x53)], 20, None),
    'face_actor': ([0, 135, N(0x47), 20, N(0x88)], 40, None),
    'wait3': ([0, 3, N(0x4A)], 20, None),
    'set_anim': ([0, 5, N(0x3D), 30, N(0x88)], 50, None),
    'play_anim': ([0, 12, N(0x3C)], 80, None),
    'play_anim0': ([0, 0, N(0x3C)], 20, None),
    'play_anim_loop': ([0, 1, N(0x3C)], 120, None),
    'play_anim_hold': ([0, 17, N(0x3C)], 200, None),
    'set_pose': ([0, 5, 2, N(0x51), 20, N(0x88)], 40, None),
    'play_anim_frames': ([0, 5, 4, 2, 1, 0, 3, N(0x59)], 80, None),
    'play_anim_frames_nowait': ([0, 5, 4, 0, 0, 0, 3, N(0x59), 40, N(0x88)], 60, None),
    'emote_wait': ([0, 2, N(0x5A)], 60, None),
    'emote': ([0, 2, N(0x5B), 40, N(0x88)], 60, None),
    'item_get': ([0, 3, N(0x58)], 90, None),
    'give_item': ([3, N(0x80)], 90, None),
    'hit_flash': ([0, N(0x43), 30, N(0x88)], 40, None),
    'tint': ([0, 255, 0, 0, 0, N(0x52), 20, N(0x88), 0, 0, 0, 0, 0, N(0x52), 10, N(0x88)], 50, None),
    'shake': ([0, 30, 60, N(0x57)], 90, None),
    'screen_flash': ([0, 255, 255, 255, 20, N(0x4C)], 40, None),
    'puff': ([0, N(0x3F), 30, N(0x88)], 40, None),
    'puff_offset': ([0, 10, -10, N(0x40), 30, N(0x88)], 40, None),
    'puff_at': ([0, 150, 140, N(0x48), 30, N(0x88)], 40, None),
    'queue_sfx': ([N(0x38), 0, 20, N(0x4A), 0, 5, N(0x4B), N(0x3A)], 40, None),
    'fire_wave_beam': ([0, 3, 20, N(0x3B)], 80, None),
    'fire_beam': ([0, 3, 30, N(0x46)], 100, None),
    'fire_beam_alt': ([0, 3, 30, N(0x5C)], 100, None),
    'fire_blast': ([0, 3, 30, N(0x5E)], 100, None),
    'fire_ball_at': ([0, 200, 152, N(0x3E)], 100, None),
    'shoot_at': ([0, 200, 152, 300, N(0x42)], 100, None),
    'shoot_at_actor': ([0, 135, 300, N(0x41)], 100, None),
    # spawned actors
    'spawn_walk_remove': ([149, 120, 150, N(0x35), 149, 160, 150, 100, N(0x44), 149, N(0x4D)], 120, None),
    'change_sprite': ([149, 120, 150, N(0x35), 149, 154, N(0x4E), 154, 150, 150, 100, N(0x44)], 120, None),
    'npc_walk': ([135, 100, 210, 100, N(0x44)], 120, None),
    # director
    'batch_two': ([N(0x38), 0, 184, 152, 200, N(0x44), 135, 100, 210, 100, N(0x44), N(0x3A)], 120, None),
    'batch_same_track': ([N(0x38), 0, 184, 152, 200, N(0x44), 0, 3, N(0x4A), N(0x3A)], 80, None),
    'group_wait': ([N(0x38), 0, 160, 152, 200, N(0x44), 135, 140, 210, 100, N(0x44), 0, N(0x39), N(0x3A),
                    20, N(0x88), 135, 66, 210, 100, N(0x44)], 200, None),
    # camera
    'cam_center_on': ([135, 2, N(0x31), 20, N(0x88)], 120, None),
    'cam_scroll_to': ([60, 120, 2, N(0x32), 20, N(0x88)], 120, None),
    'cam_follow': ([135, N(0x33), 135, 100, 260, 100, N(0x44), 30, N(0x88)], 200, None),
    'cam_release': ([N(0x34), 0, 184, 152, 200, N(0x44), 30, N(0x88)], 120, None),
    'cam_follow_wait': ([135, 30, N(0x6A), 20, N(0x88)], 120, None),
    'cam_pan_to': ([60, 120, 200, N(0x6B), 20, N(0x88)], 120, None),
    'cam_release_wait': ([20, N(0x6C), 0, 184, 152, 200, N(0x44)], 80, None),
    'cam_set': ([50, 90, N(0x6F), 20, N(0x88)], 40, None),
    'cam_set3': ([50, 90, 0, N(0x70), 20, N(0x88)], 40, None),
    'cam_param8': ([5, N(0x6D), 20, N(0x88)], 40, None),
    'cam_param12': ([5, N(0x6E), 20, N(0x88)], 40, None),
    # screen
    'fade_out_in': ([16, N(0x78), 10, N(0x88), 16, N(0x79)], 80, None),
    'fade_white': ([16, N(0x7A), 10, N(0x88), 16, N(0x7B)], 80, None),
    'mosaic': ([16, N(0x7C), 10, N(0x88), 16, N(0x7D)], 80, None),
    'flash_white': ([10, N(0x7E)], 40, None),
    'hud': ([N(0x30), 40, N(0x88), N(0x2F), 40, N(0x88)], 100, None),
    'boss_bar': ([135, N(0x8F), 40, N(0x88), N(0x91), 30, N(0x88)], 100, None),
    'title_text': ([100, 120, 80, N(0xB2), 30, N(0x88), 10, N(0xEF)], 80, None),
    'bg_layers': ([1, N(0x26), 10, N(0x88), 1, N(0x25), 1, 2, N(0x27), 1, 3, N(0x28), 50, 50, N(0x2A), 10, N(0x88)], 40, None),
    # waits / input
    'wait10': ([10, N(0x88)], 20, None),
    'wait_button': ([N(0x89)], 40, keys_at(12)),
    'a_pressed': ([N(0xD2), 4, N(0x88), N(0xD2)], 20, keys_at(0)),
}

# full scenarios: name -> dict(state, event | None (natural), params, frames, keys, pre, how); see full_scenarios.py
from full_scenarios import FULL, record_full  # noqa: E402


def run_micro(names=None):
    for name, (items, frames, keys) in MICRO.items():
        if names and name not in names:
            continue
        e, doc = record_code(SW, asm(items), frames=frames, keys=keys)
        doc['items'] = [list(i) if isinstance(i, tuple) else (i.hex() if isinstance(i, bytes) else i) for i in items]
        p = save(f'micro/{name}', doc, {'state': SW})
        print(name, 'done', doc['done'], p.name)


def run_full(names=None):
    for name, s in FULL.items():
        if names and name not in names:
            continue
        e, doc = record_full(s)
        p = save(name, doc, {'state': str(s['state']), 'how': s.get('how', '')})
        print(name, 'event', doc.get('event'), 'done', doc['done'], len(doc['frames']), p.name)


if __name__ == '__main__':
    args = sys.argv[1:] or ['micro']
    if args == ['micro']:
        run_micro()
    elif args == ['full']:
        run_full()
    elif args == ['all']:
        run_micro()
        run_full()
    else:
        if any(a in MICRO for a in args):
            run_micro([a for a in args if a in MICRO])
        run_full([a for a in args if a in FULL])
