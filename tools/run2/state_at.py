"""state_at.py ROW OUT.ss [--chain run2]: the emulator state before chain row ROW, saved to OUT.ss - replayed from
the chain's nearest cut state (<chain>/cuts/c<N>.ss, N <= ROW) with the recorded keys (headless, muted). For making
menu_parity / route-planning / tour states somewhere a real run passes (e.g. the Capsule Corp lobby, run 2 row
74,990). A save state does not carry the cartridge's EEPROM (the in-game saves): it is taken from the cut's segment
doc (c<N>.json init.eeprom), kept through the replay and written next to the state as OUT.ss.eeprom (raw bytes;
tools/segments/tours.py loads it and records it in the segment, as the whole-run cuts do)."""
import base64
import json
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/superbuu', 'tools/wholerun', 'tools/story_playthrough'):
    sys.path.insert(0, str(ROOT / d))
from sbemu import sd_read, sd_write  # noqa: E402  (the savedata-capable emulator lib first)
import chains  # noqa: E402
from route import start_emu, status  # noqa: E402

a = sys.argv[1:]
row, out = int(a[0]), a[1]
chain = chains.chain_arg(a) if '--chain' in a else 'run2'
P = chains.paths(chain)
cuts = sorted(int(p.stem[1:]) for p in P['cuts'].glob('c*.ss') if p.stem[1:].isdigit() and int(p.stem[1:]) <= row)
keys = []
for _, ks, _ in chains.parts(chain):
    keys += ks
start = cuts[-1] if cuts else 0
if not start:
    sys.exit('no cut state at or before the row (power-on replay not supported here)')
e = start_emu(str(P['cuts'] / f'c{start}.ss'))
doc = P['cuts'] / f'c{start}.json'
eeprom = None
if doc.exists():
    # (the doc's head only: the init comes before the frames)
    head = doc.read_text()
    i = head.find('"eeprom":"')
    if i >= 0:
        j = head.find('"', i + 10)
        eeprom = zlib.decompress(base64.b64decode(head[i + 10:j]))
        sd_write(e, eeprom)
for r in range(start, row):
    e.set_keys(keys[r])
    e.run(1)
e.set_keys(0)
e.save_file(out)
sd = sd_read(e)
if sd:
    Path(out + '.eeprom').write_bytes(sd)
print(f'{chain} row {row} (from cut {start}, eeprom {"yes" if eeprom else "no"}) -> {out}', status(e))
