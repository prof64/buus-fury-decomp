"""rec_worldmap.py [NAME ...] [--print] -- world map (area 0-50) ground truth (fork E, re/buu/notes.md "Field: world map").

Setup (buu_snakeway_start.ss): warp to Southwest Forest 8-1 (re/buu/warp.py trick, load #1), place the player on the
capsule (280, 275) facing up, press A: the capsule trigger runs inline_0x0807AAAC -> event s515 (pose, glide up,
warp_fade(0, 50, 0, 872, 1800)) -> EnterArea into the world map (load #2). Frame 0 = the first logic frame after that
load (common_field.record). Per frame, besides the common rows: `fly` = the flying player (x, y 24.8, z 22.10,
heading h 22.10 angle (256 << 10 = full turn), facing, raw anim, anim frame, flight state rev / spd / turn) and `cam3d`
= the map object 0x030019A4 (+0 x, +4 y, +8 height (22.10), +0xC heading, +0x10 pitch). Screenshots of some frames
go to work/buu/wm_<name>_<f>.png (renderer reference). Writes data/field/worldmap_<name>.json."""
import json
import sys
from wm_lib import *  # noqa
from common import ROOT
import common_field as cf

SHOTS = {0, 30, 120, 300}


def setup_capsule_8_1(e):
    cf.arm_warp(e, 8, 1, 280, 290)
    st = {'phase': 0, 'n': 0}

    def pre(e):
        a = area(e)
        if st['phase'] == 0:
            cf.follow_warp(e)
            e.set_keys(KEY.RIGHT)
            if a[:2] == [8, 1] and e.r32(PLAYER):
                st['phase'] = 1
        elif st['phase'] == 1:
            # settle in 8-1 (fade-in), then place on the capsule facing up
            e.set_keys(0)
            st['n'] += 1
            if st['n'] == 90:
                p = e.r32(PLAYER)
                e.w32(p + 0x1A0, 280 << 8)
                e.w32(p + 0x1A4, 275 << 8)
                e.w8(p + 0x10, 1)
            if st['n'] == 94:
                e.set_keys(KEY.A)
                st['phase'] = 2
        else:
            e.set_keys(0)
    return pre


def U(*a):
    return [[k, n] for k, n in a]


SCENARIOS = [
    {'name': 'flight', 'setup': setup_capsule_8_1,
     'notes': 'world map 0-50 from the 8-1 capsule: thrust (UP), turns (LEFT / RIGHT, banking anim), reverse (DOWN), '
              'climb (B), descend (A), coasting; no landing',
     'input': U((0, 20), (KEY.UP, 80), (KEY.UP | KEY.RIGHT, 40), (KEY.UP, 40), (KEY.UP | KEY.LEFT, 60), (0, 40),
                (KEY.B, 50), (KEY.DOWN, 50), (KEY.DOWN | KEY.RIGHT, 30), (0, 30), (KEY.A, 40), (KEY.UP | KEY.B, 60),
                (KEY.A | KEY.B, 20), (0, 40))},
    {'name': 'land_8_1', 'setup': setup_capsule_8_1,
     'notes': 'world map arrival over the Southwest Forest landing spot (872, 1800): a short hop forward and back, '
              'then A until the altitude trigger (0 < z < 16 px, rect 849,1785-880,1808, inline_0x08073424) fires and '
              'warps to 8-1 (288, 168)',
     'input': U((0, 20), (KEY.UP, 6), (0, 30), (KEY.DOWN, 6), (0, 40), (KEY.A, 60), (0, 400))},
]


def fly_row(e):
    r = flyer(e)
    if r is None or r['vt'] != '0804E1B4':
        return None
    return [r['x'], r['y'], r['z'], r['h'], r['facing'], r['raw'], r['aframe'], r['rev'], r['spd'], r['turn']]


def cam3d(e):
    return [s32v(e.r32(MAPOBJ + o)) for o in (0, 4, 8, 0xC, 0x10)]


def run(scn, show=False):
    # record() drives the frames; wrap run_frame to add the world-map fields + screenshots
    orig = cf.run_frame
    rows = []
    shot_dir = WORK

    def rf(e):
        info, hs = orig(e)
        rows.append((fly_row(e), cam3d(e), e.r32(FC)))
        return info, hs
    cf.run_frame = rf
    try:
        t = cf.record(scn['name'], 'buu_snakeway_start.ss', scn['setup'], scn['input'], scn['notes'], max_setup=900,
                      frame0_load=2)
    finally:
        cf.run_frame = orig
    # the last len(frames) rows belong to the recorded frames
    tail = rows[-len(t['frames']):]
    for fr, (fly, cam, fc) in zip(t['frames'], tail):
        fr['fly'] = fly
        fr['cam3d'] = cam
    t['init']['map'] = {'w': 3584, 'h': 2560, 'note': 'map object +0x94 / +0x98: the flyer wraps around'}
    t['format_extra'] = {'fly': '[x 24.8, y 24.8, z 22.10, heading 22.10 (256<<10 = turn), facing, raw anim, anim frame, '
                                'state +4 rev, +8 speed, +0xC turn rate]',
                         'cam3d': '[x 22.10, y 22.10, height 22.10, heading, pitch] (map object 0x030019A4)'}
    out = ROOT / 'data' / 'field' / f"worldmap_{scn['name']}.json"
    json.dump(t, open(out, 'w'), separators=(',', ':'))
    if show:
        for fr in t['frames']:
            print(fr['f'], fr['logic'], fr['area'], fr['phase'], fr['p'], fr['fly'], fr['cam3d'], fr['rng'])
    print(out, len(t['frames']), 'frames; loads', [(L['f'], L['to'], L['dest']) for L in t['loads']])
    return t


if __name__ == '__main__':
    names = [a for a in sys.argv[1:] if not a.startswith('-')]
    for s in SCENARIOS:
        if not names or s['name'] in names:
            run(s, '--print' in sys.argv)
