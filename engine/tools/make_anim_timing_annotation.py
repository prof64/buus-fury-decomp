"""Condense tools/extract/data/anim_obs_buu.json (emulator animation observations) into the annotation the Rust
extractor embeds: engine/annotations/anim_timing_measured.json =
  {"<anim struct address hex>": {"medians": {"<frame>": duration}, "count": segments, "observed_sequences": [[seq, n], ...]}}
Same reduction as tools/extract/sprites_wf_export.load_timing (median = sorted(lst)[len // 2], top-3 sequences).
usage: .venv/bin/python engine/tools/make_anim_timing_annotation.py
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
src = json.loads((ROOT / 'tools/extract/data/anim_obs_buu.json').read_text())
out = {}
for addr, v in src.items():
    rec = v.get('-')
    if not rec or not rec.get('dur'):
        continue
    med = {k: sorted(lst)[len(lst) // 2] for k, lst in rec['dur'].items() if lst}
    out[addr] = {'medians': med, 'count': rec['count'],
                 'observed_sequences': sorted(rec['seqs'].items(), key=lambda kv: -kv[1])[:3]}
(ROOT / 'engine/annotations/anim_timing_measured.json').write_text(json.dumps(out, indent=0, sort_keys=True))
print(len(out), 'animations')
