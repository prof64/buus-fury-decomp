"""power_on.py [OUT.json]: the sound hardware at the whole run's start (the soft reset, `route.start_emu('reset')`): the
sound driver's RAM (identical after every soft reset - set by the boot code: checked against resets from other save
states, work/side/probe/sndreset.py), the play position / last update and the timer phase to the first DMA interrupt
(the hardware's timing at the reset moment - two numbers, not game state). The app's power-on (`wholerun --power-on`)
starts its sound model from these; nothing else of the RAM is used. Default OUT work/wholerun/power_on.json."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from route import start_emu  # noqa: E402

out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'work' / 'wholerun' / 'power_on.json'
e = start_emu('reset')
dram = e.read(0x030038A4, 0x18 + 15 * 0x60) + e.read(0x03003898 + 0x5C4, 16 * 0x40)
pos, last = e.r32(0x0300389C), e.r32(0x030038A0)
c0 = e.cycles()
while e.r32(0x0300389C) == pos:
    e.step()
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps({'driver': dram.hex(), 'pos': pos, 'last': last, 'phase': e.cycles() - c0}))
print(out, 'pos', pos, 'last', last, 'phase', e.cycles() - c0)
