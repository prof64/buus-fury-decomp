"""Rebuild every graphics output for all three games: everything the dashboard and the battle simulator load.

    .venv/bin/python tools/extract/extract_all.py [game ...] [--skip-maps] [--no-icons] [--no-audio]

Outputs: site/public/assets/<game>/{sprites,effects,portraits,...}/<entity>/{NNN.png,sheet.png}
         site/public/assets/<game>/battle/...   battle HUD icons (tools/battle/extract_icons.py, log1_icons.py,
                                                 extract_buu_icons.py; run after the game's assets were rebuilt)
         data/<game>/sprites.json
         site/public/assets/buu/audio/ + data/buu/audio.json   music + SFX (tools/audio/rip_buu.py; renders are cached
                                                 in work/audio/cache, so a rebuild only re-encodes)
Finishes with tools/check_assets.py (every asset path referenced by data/ and site/src exists).
"""
import sys
import shutil
import time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from gba import ASSETS, DATA, GAMES  # noqa: E402
from export import write_json  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent.parent
# battle HUD icons per game (the rebuild above wipes site/public/assets/<game>)
ICON_SCRIPTS = {'log1': 'log1_icons.py', 'log2': 'extract_icons.py', 'buu': 'extract_buu_icons.py'}


def battle_icons(game):
    import subprocess
    script = ROOT / 'tools' / 'battle' / ICON_SCRIPTS[game]
    print(game, 'battle icons:', script.name, flush=True)
    subprocess.run([sys.executable, str(script)], cwd=ROOT, check=True)


def audio(game):
    import subprocess
    script = ROOT / 'tools' / 'audio' / f'rip_{game}.py'
    if script.exists():
        print(game, 'audio:', script.name, flush=True)
        subprocess.run([sys.executable, str(script)], cwd=ROOT, check=True)


def run(game, maps=True):
    t = time.time()
    extra = {}
    if maps:
        shutil.rmtree(ASSETS / game, ignore_errors=True)
    else:   # keep previously rendered maps (slow, emulator-driven) and their json
        import json
        old = DATA / game / 'sprites.json'
        if old.exists():
            prev = json.loads(old.read_text())
            extra = {k: prev[k] for k in ('maps', 'tilesets', 'maps_note') if k in prev}
        if (ASSETS / game).exists():
            for d in (ASSETS / game).iterdir():
                if d.name not in ('maps', 'audio'):   # audio: tools/audio/rip_buu.py output
                    shutil.rmtree(d, ignore_errors=True)
    ents = []
    if game == 'log1':
        import sprites_log1_export
        import log1_misc
        ents += sprites_log1_export.export()
        ents += log1_misc.all_assets(extra)
    else:
        import sprites_wf_export
        import misc_export
        from wf_sprites import WFSprites
        ents += sprites_wf_export.export(game)
        defs = WFSprites(game).all_defs()
        ents += misc_export.effects(game, defs)
        if game == 'buu':
            ents += misc_export.buu_overlays()
        ents += misc_export.portraits(game)
        import items_export
        ents += items_export.export(game)
    import ui_export
    extra.update(ui_export.export(game))
    if maps:
        import tiles_export
        extra.update(tiles_export.export(game))
    doc = {'game': game, 'generated_by': 'tools/extract/extract_all.py', 'format_doc': 'tools/extract/GRAPHICS.md',
           'conventions': {
               'directions': 'down, up, left, right (engine direction index 0..3); "any" for direction-less',
               'anchor': 'pixel inside the frame PNG that sits on the entity position (x,y)',
               'flip': 'true = draw the referenced frame mirrored horizontally around its anchor',
               'duration_frames': '60 Hz game frames; timing_source tells whether measured or default'},
           'entities': ents}
    doc.update(extra)
    write_json(game, 'sprites.json', doc)
    cats = {}
    for e in ents:
        cats[e['category']] = cats.get(e['category'], 0) + 1
    print(game, len(ents), 'entities', cats, f'{time.time() - t:.1f}s', flush=True)


if __name__ == '__main__':
    import subprocess
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    for g in (args or GAMES):
        run(g, maps='--skip-maps' not in sys.argv)
        if '--no-icons' not in sys.argv:
            battle_icons(g)
        if '--no-audio' not in sys.argv:
            audio(g)
    sys.exit(subprocess.run([sys.executable, str(ROOT / 'tools' / 'check_assets.py')]).returncode)
