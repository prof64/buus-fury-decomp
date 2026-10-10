"""music_record.py: the game's music over the link flows, from two linked emulators (headless, muted) - the host's
sound object after every video frame (the player's playing byte 0x030038A4 + 5 and its song, +0xC: the header's index
in the song table). Writes the tracked data/link/music.json: per flow `changes` [[video frame, logic frame, playing,
song], ...] (the frames where the state changed, from the reset) - what wf-app tests/link.rs holds the app's song
requests against.

  fight_then_back  recording dojo_px3's keys (the pages, a 3-round In-Game fight, the character page again), then the
                   host's B three times: back to the setup page, Select Saved Game, the title's Multiplayer page
  lost_on_page     the host's setup page; the guest's console reset (the link error's script s001, A at its box, the
                   title)
  lost_in_fight    dojo_px3's keys into the first round, then the guest's reset

    .venv/bin/python tools/link/music_record.py
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'menu'))
import linkcap as lc  # noqa: E402
import scenarios_link as sl  # noqa: E402

dr = lc.dr
PLAYER, SONGS = 0x030038A4, 0x083BB78C


def state(e):
    sp = e.r32(PLAYER + 0xC)
    return [e.r8(PLAYER + 5), (sp - SONGS) // 20 if SONGS <= sp < SONGS + 53 * 20 else -1]


def flow(name):
    a, b = lc.pair()
    out, n = [], [0]

    def step(ka, kb):
        a.set_keys(ka)
        b.set_keys(kb)
        dr.step(a, b)
        s = state(a)
        if not out or out[-1][2:] != s:
            out.append([n[0], a.r32(0x03000F94)] + s)
        n[0] += 1

    px3 = json.loads((ROOT / 'data' / 'link' / 'dojo_px3.json').read_text())['video_keys']
    if name == 'fight_then_back':
        for ka, kb in px3:
            step(ka, kb)
        for i in range(900):
            step(dr.keyof('B') if i % 120 in (60, 61) else 0, 0)
        return out
    if name == 'lost_in_fight':
        for ka, kb in px3[:2600]:
            step(ka, kb)
    else:
        for ka, kb, k in sl.TO_SETUP:
            for _ in range(k):
                step(dr.keyof(ka), dr.keyof(kb))
    dr.soft_reset(b)
    for i in range(700):
        step(dr.keyof('A') if 400 <= i < 404 else 0, 0)
    return out


if __name__ == '__main__':
    res = {}
    for name in ('fight_then_back', 'lost_on_page', 'lost_in_fight'):
        # (a pair per process: the link coordinator is the process's)
        if len(sys.argv) > 1:
            print(json.dumps(flow(sys.argv[1])))
            sys.exit(0)
        import subprocess
        p = subprocess.run([sys.executable, __file__, name], capture_output=True, text=True, check=True)
        res[name] = {'changes': json.loads(p.stdout.strip().splitlines()[-1])}
        print(name, res[name]['changes'])
    (ROOT / 'data' / 'link' / 'music.json').write_text(json.dumps(res, separators=(',', ':')) + '\n')
