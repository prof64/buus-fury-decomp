"""Curated names for sprite entities and animations (tools/extract/data/names_<game>.json).

names_<game>.json = {
  "entities": { "<sprite index>": {"name", "category", "slug"?, "confidence"?, "anim_set"?, "anims"?: {"<idx>": "name"}, "notes"?} },
  "anim_sets": { "<set name>": {"<idx>": "name", ...} }   # shared animation layouts (same engine class)
}
"""
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent


def slugify(s):
    return re.sub(r'[^a-z0-9]+', '_', s.lower()).strip('_')


class Names:
    def __init__(self, game):
        p = HERE / 'data' / f'names_{game}.json'
        self.d = json.loads(p.read_text()) if p.exists() else {'entities': {}, 'anim_sets': {}}
        self.used = {}

    def entity(self, idx):
        e = self.d['entities'].get(str(idx), {})
        name = e.get('name') or f'Sprite {idx}'
        slug = e.get('slug') or f'{idx:03d}_{slugify(name)}'
        return {'name': name, 'slug': slug, 'category': e.get('category', 'npc'),
                'confidence': e.get('confidence', 'low' if 'name' not in e else 'medium'), 'notes': e.get('notes')}

    def anim_name(self, idx, ai, nframes):
        e = self.d['entities'].get(str(idx), {})
        nm = (e.get('anims') or {}).get(str(ai))
        if not nm and e.get('anim_set'):
            nm = self.d['anim_sets'].get(e['anim_set'], {}).get(str(ai))
        if not nm:
            nm = f'anim{ai:02d}'
        # keep unique
        key = (idx, nm)
        if key in self.used and self.used[key] != ai:
            nm = f'{nm}_{ai:02d}'
        self.used[(idx, nm)] = ai
        return nm

    def default_timing(self, idx, ai):
        e = self.d['entities'].get(str(idx), {})
        t = (e.get('timing') or {}).get(str(ai))
        if t:
            return t
        if e.get('anim_set'):
            return self.d.get('set_timing', {}).get(e['anim_set'], {}).get(str(ai))
        return None


def load(game):
    return Names(game)
