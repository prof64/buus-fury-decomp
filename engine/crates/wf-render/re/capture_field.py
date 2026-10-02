"""Field reference videos for the frontend (headless, muted): re-runs the field recorders' scenarios
(tools/field/rec_transitions.py, rec_npc.py, rec_objects.py — the same traces wf-core's field lockstep replays) and
keeps the emulator screen of every recorded video frame.

  .venv/bin/python engine/crates/wf-render/re/capture_field.py [transitions:forest_roundtrip npc:yemma_crowd ...]

writes work/frontend/field/<kind>_<name>.json (the recording, same format as data/field/) and
work/frontend/field/<kind>_<name>/NNNN.png (video frame NNNN of the recording). ROM-derived: git-ignored work/.
Used by `cargo test -p wf-app --test field_parity`.
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
sys.path.insert(0, str(ROOT / 're' / 'buu'))
sys.path.insert(0, str(ROOT / 'tools' / 'emu'))
import emu as emu_mod  # noqa: E402
from PIL import Image  # noqa: E402

OUT = ROOT / 'work' / 'frontend' / 'field'
OUT.mkdir(parents=True, exist_ok=True)

DEFAULT = ['transitions:forest_roundtrip', 'transitions:hercule_edge_up', 'npc:yemma_crowd', 'npc:westcity_idle',
           'objects:item_8_57', 'objects:breakable_drops_8_1']

# every emulated video frame's screen, by the emulator's frame number (a rewind + re-run overwrites)
SHOTS = {}
_run = emu_mod.Emu.run


def run(self, frames=1):
    for _ in range(frames):
        _run(self, 1)
        SHOTS[self.frame] = (self.pixels(), self.oam(), self.r32(0x03001C34))


emu_mod.Emu.run = run


def main():
    want = sys.argv[1:] or DEFAULT
    for item in want:
        kind, name = item.split(':')
        SHOTS.clear()
        if kind == 'transitions':
            import rec_transitions as m
            from common_field import record
            sc = next(s for s in m.SCENARIOS if s['name'] == name)
            t = record(sc['name'], sc['state'], sc['setup'], sc['input'], sc['notes'])
        elif kind == 'npc':
            import rec_npc as m
            sc = next(s for s in m.SCENARIOS if s['name'] == name)
            t = m.record(sc)
        elif kind == 'objects':
            import rec_objects as m
            sc = next(s for s in m.SCENARIOS if s['name'] == name)
            t = m.record(sc, str(OUT))
        else:
            raise SystemExit(f'unknown kind {kind}')
        n = len(t['frames'])
        frames = sorted(SHOTS)[-n:]
        d = OUT / f'{kind}_{name}'
        d.mkdir(exist_ok=True)
        oams = []
        for k, fr in enumerate(frames):
            px, oam, _ = SHOTS[fr]
            Image.frombytes('RGB', (240, 160), px).save(d / f'{k:04d}.png')
            oams.append(oam.hex())
        # OAM after each frame (= the build the *next* frame shows: shadow OAM is copied at VBlank)
        (d / 'oam.json').write_text(json.dumps(oams))
        (OUT / f'{kind}_{name}.json').write_text(json.dumps(t, separators=(',', ':')))
        print(item, n, 'frames ->', d.relative_to(ROOT))


if __name__ == '__main__':
    main()
