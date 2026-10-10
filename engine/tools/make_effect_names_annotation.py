"""Snapshot the curated effect names (tools/extract/buu_fx.run_names(), merged with names_buu.json `effects` like
misc_export.effects) into engine/annotations/effect_names.json = {"<first record addr hex>": {name, confidence, notes}},
and re-copy names_buu.json to engine/annotations/sprite_names.json.
usage: .venv/bin/python engine/tools/make_effect_names_annotation.py
"""
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'extract'))
import buu_fx  # noqa: E402

names = json.loads((ROOT / 'tools/extract/data/names_buu.json').read_text())
fx = {**buu_fx.run_names(), **names.get('effects', {})}
(ROOT / 'engine/annotations/effect_names.json').write_text(json.dumps(fx, indent=1, sort_keys=True))
shutil.copy(ROOT / 'tools/extract/data/names_buu.json', ROOT / 'engine/annotations/sprite_names.json')
print(len(fx), 'effect names')
