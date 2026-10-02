"""The game's RNG calls (caller, kind, argument) in chosen frames of the recorded playthrough: replays the recorded
keys from the reset and logs rand / chance / next in frames FROM..TO (recording frame numbers).

usage: python3 tools/story_playthrough/draws.py FROM TO [data/story_playthrough/opening.json]"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from route import start_emu, intro_keys  # noqa: E402
from common_field import SoundLog  # noqa: E402


def main():
    a, b = int(sys.argv[1]), int(sys.argv[2])
    p = Path(sys.argv[3]) if len(sys.argv) > 3 else HERE.parent.parent / 'data' / 'story_playthrough' / 'opening.json'
    doc = json.loads(p.read_text())
    f0 = doc['skip']
    e = start_emu('reset')
    for f in range(f0):
        e.set_keys(intro_keys(f))
        e.run(1)
    log = None
    for k, fr in enumerate(doc['frames']):
        if k > b:
            break
        if k == a:
            log = SoundLog(e)
        e.set_keys(fr['keys'])
        e.run(1)
        if log:
            hs = e.hits()
            print(k, fr['lf'], [d[:3] for d in log.draws(hs)])
    if log:
        log.close()


if __name__ == '__main__':
    main()
