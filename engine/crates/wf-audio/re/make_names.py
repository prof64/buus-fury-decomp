"""Copy the curated song / SFX names (our own research, data/buu/audio.json) into the crate's embedded
annotation file engine/crates/wf-audio/data/names.json. Re-run when the catalog changes."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
cat = json.loads((ROOT / 'data' / 'buu' / 'audio.json').read_text())
out = {
    'source': 'data/buu/audio.json (tools/audio/names_buu.py): names from the area table, story scripts and '
              'emulator breakpoints on PlaySfx; curated, not ROM data',
    'songs': {str(m['id']): {k: m.get(k) for k in ('name', 'context', 'confidence', 'areas', 'ends')} for m in cat['music']},
    'sfx': {str(s['id']): {'name': s['name'], 'trigger': s.get('trigger', ''), 'confidence': s.get('confidence'),
                           'game_volume': s.get('game_volume', 64)} for s in cat['sfx']},
    'arena_music': cat.get('arena_music'),
    'boss_music': cat.get('boss_music'),
    'events': cat.get('events'),
}
p = Path(__file__).resolve().parents[1] / 'data' / 'names.json'
p.parent.mkdir(exist_ok=True)
p.write_text(json.dumps(out, indent=1, ensure_ascii=False) + '\n')
print('wrote', p, len(out['songs']), 'songs', len(out['sfx']), 'sfx')
