"""wm_model.py [NAME ...] -- the world-map flight model (spec for the Rust port), replayed against
data/field/worldmap_<name>.json: flyer x / y / z / heading / facing / raw anim / anim frame / flight state, the 3D
camera (x, y, height, heading, pitch) and the RNG (no draws while flying), every logic frame.

Per logic frame (actor update 0x0800EF08 of the flyer, then the camera object 0x08034124):
  state tick 0x08010398: tick16 += 1; frame = (tick >> 3) & 3
  control 0x080103A6 (state obj: +4 rev, +8 spd, +0xC turn; held keys):
    A held: z > 5<<10 -> z -= 1<<10      else B held: z < 120<<10 -> z += 1<<10
    acc = RIGHT ? 0x80 : LEFT ? -0x80 : 0
    UP: spd += 0xC0 / DOWN: spd -= 0xC0;  spd = tdiv(spd * 15, 16)
    rev = 0 if spd > 0, 1 if spd < 0 (else kept);  if rev: acc = -acc
    turn += acc; turn -= turn >> 3 (asr); if turn > 0: turn -= 1
    h += turn
    x += (SIN[(h >> 10) & 255] * spd) >> 18;  y -= (SIN[((h >> 10) + 64) & 255] * spd) >> 18
    wrap 0x0800FEE2: x < 0 -> x += W << 8, else (x >> 8) >= W -> x -= W << 8; same y with H (W, H = 3584, 2560)
    anim: turn > 0x80 -> raw 17, facing = rev ? 2 : 1; turn < -0x80 -> raw 17, facing = rev ? 0 : 3;
          else raw 16, facing = rev ? 0 : 1   (SetAnimRaw 0x0800ECDA k: raw = k + 1: 0xF -> 16, 0x10 -> 17)
  camera 0x08034124 (map object 0x030019A4): +0xC = h; if z changed: pitch S = (z*218 + tdiv(z*z, 3511) + 0x10F3333) >> 10
    (64-bit); sI(a) = (SIN[(a>>10)&255] * (1024 - (a & 1023)) + SIN[((a>>10)+1)&255] * (a & 1023)) >> 16, cI(a) = sI(a + (64<<10))
    cam.x = (x << 2) - ((sI(h) * cI(S)) * 30 >> 10); cam.y = (y << 2) + ((cI(h) * cI(S)) * 30 >> 10); cam.h = z + sI(S) * 30
  arrival (EnterArea on the 3D map): flyer at dest, z = 60 << 10, h = 0, state zero; fade-in of 64 logic frames during
    which the control runs; at the end of the 64th the flyer's SetAnim(0) installs a fresh state (rev = spd = turn = 0,
    tick 0, frame 0, facing 1); position kept."""
import json
import sys
from wm_lib import *  # noqa
from common import ROOT, s32 as rs32

SIN = [rs32(0x087B6EEC + 4 * i) for i in range(256)]
W, H = 3584, 2560
FADE_IN = 64


def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def asr(v, n):
    return v >> n


def sI(a):
    i = (a >> 10) & 255
    f = a & 1023
    return (SIN[i] * (1024 - f) + SIN[(i + 1) & 255] * f) >> 16


def cI(a):
    return sI(a + (64 << 10))


def pitch(z):
    return (z * 218 + tdiv(z * z, 3511) + 0x10F3333) >> 10


class Flyer:
    def __init__(self, x, y, z):
        self.x, self.y, self.z, self.h = x, y, z, 0
        self.rev = self.spd = self.turn = 0
        self.tick = self.frame = 0
        self.facing, self.raw = 1, 16
        self.cam = None
        self.zc = None
        self.S = 0

    def step(self, keys):
        self.tick = (self.tick + 1) & 0xFFFF
        self.frame = (self.tick >> 3) & 3
        if keys & KEY.A:
            if self.z > 5 << 10:
                self.z -= 1 << 10
        elif keys & KEY.B:
            if self.z < 0xF << 13:
                self.z += 1 << 10
        acc = 0x80 if keys & KEY.RIGHT else (-0x80 if keys & KEY.LEFT else 0)
        if keys & KEY.UP:
            self.spd += 0xC0
        elif keys & KEY.DOWN:
            self.spd -= 0xC0
        self.spd = tdiv(self.spd * 15, 16)
        if self.spd > 0:
            self.rev = 0
        elif self.spd < 0:
            self.rev = 1
        if self.rev:
            acc = -acc
        t = self.turn + acc
        t -= t >> 3
        if t > 0:
            t -= 1
        self.turn = t
        self.h += t
        self.x += (SIN[(self.h >> 10) & 255] * self.spd) >> 18
        self.y -= (SIN[((self.h >> 10) + 64) & 255] * self.spd) >> 18
        if self.x < 0:
            self.x += W << 8
        elif (self.x >> 8) >= W:
            self.x -= W << 8
        if self.y < 0:
            self.y += H << 8
        elif (self.y >> 8) >= H:
            self.y -= H << 8
        if self.turn > 0x80:
            self.raw, self.facing = 17, (2 if self.rev else 1)
        elif self.turn < -0x80:
            self.raw, self.facing = 17, (0 if self.rev else 3)
        else:
            self.raw, self.facing = 16, (0 if self.rev else 1)

    def camera(self):
        if self.z != self.zc:
            self.zc = self.z
            self.S = pitch(self.z)
        cs = cI(self.S)
        cx = (self.x << 2) - ((sI(self.h) * cs * 30) >> 10)
        cy = (self.y << 2) + ((cI(self.h) * cs * 30) >> 10)
        ch = self.z + sI(self.S) * 30
        self.cam = [cx, cy, ch, self.h, self.S]

    def reset_state(self):
        self.rev = self.spd = self.turn = 0
        self.tick = self.frame = 0
        self.facing = 1


def area_triggers():
    """0-50's list-0 triggers in scan order (exported order), with their activation"""
    o = json.load(open(ROOT / 'engine/target/tmp/field-content/base/maps/area_00_050/objects.json'))
    out = []
    for x in o['areas'][0]['objects']:
        if 'rect' not in x:
            continue
        r = x['rect']
        act = x['activation']
        alt = (act['z_min'], act['z_max']) if act['mode'] == 'touch_altitude' else None
        out.append((x['type'], (r['x'], r['y'], r['x'] + r['w'], r['y'] + r['h']), alt, x.get('script') or x.get('message') or x.get('event'), x.get('condition')))
    return out


def trigger(m, trigs):
    """the trigger check 0x080126D4 on the flyer: point (x >> 8, y >> 8), first match fires, scan stops; altitude =
    lo << 10 < z < hi << 10 (exclusive). Conditions (flags) not evaluated here: only always-present triggers."""
    px, py = m.x >> 8, m.y >> 8
    for kind, r, alt, what, cond in trigs:
        if cond and 'always' not in cond:
            continue
        if not (r[0] <= px < r[2] and r[1] <= py < r[3]):
            continue
        if alt and not ((alt[0] << 10) < m.z < (alt[1] << 10)):
            continue
        return kind, what
    return None


def run(name):
    t = json.load(open(ROOT / 'data' / 'field' / f'worldmap_{name}.json'))
    trigs = area_triggers()
    keys = []
    for k, n in t['input']:
        keys += [k] * n
    fr0 = t['frames'][0]
    x, y = t['init']['player']['x'], t['init']['player']['y']
    m = Flyer(x, y, 60 << 10)
    nlog = 0
    ok = 0
    frames = t['frames']
    label = None
    for k, fr in enumerate(frames):
        if not fr['logic']:
            continue
        if fr['fly'] is None:
            break
        m.step(keys[fr['f']])
        m.camera()
        nlog += 1
        if nlog == FADE_IN:
            m.reset_state()
        got = [m.x, m.y, m.z, m.h, m.facing, m.raw, m.frame, m.rev, m.spd, m.turn]
        # the camera object runs at the end of the iteration: when the iteration spilled into the next video frame
        # (a non-logic frame follows) the recorded camera is that next frame's
        cam = frames[k + 1]['cam3d'] if k + 1 < len(frames) and not frames[k + 1]['logic'] else fr['cam3d']
        tr = trigger(m, trigs) if nlog > FADE_IN else None
        if tr and tr[0] == 'message_trigger':
            if tr[1] != label:
                label = tr[1]
                print(f'   frame {fr["f"]}: label message {label}')
            tr = None
        if got != fr['fly'] or m.cam != cam:
            print(f'{name}: first divergence at frame {fr["f"]} (logic #{nlog})')
            print('   emu', fr['fly'], fr['cam3d'])
            print('   sim', got, m.cam)
            return False
        ok += 1
        if tr:
            nxt = [f for f in frames[k + 1:] if f['logic']][:1]
            print(f'   frame {fr["f"]}: {tr[0]} {tr[1]} fires (the script takes over; next logic frame z {nxt[0]["fly"] and nxt[0]["fly"][2]})')
            break
    print(f'{name}: {ok} logic frames identical (flyer + camera)')
    return True


if __name__ == '__main__':
    for n in sys.argv[1:] or ['flight', 'land_8_1']:
        run(n)
