"""The REAL playthrough (ship -> credits, no RAM writes) as one chain of key logs.

usage: realrun.py pack [JOBS]          verify every stretch of CHAIN against its local states / key logs under work/
                                       (replay the keys from the stretch's start state, find the frame where the
                                       game is in the end state) and write data/realrun/NNN_<name>.keys.gz + manifest.json
       realrun.py rebuild OUTDIR [FIRST.ss]   replay the packed logs in order from the first state (default: the
                                       ship's start, work/ship/real4/start.ss); OUTDIR/NNN_<name>.ss at every stretch
                                       end, each checked against the manifest's end status
       realrun.py runs OUTDIR          OUTDIR/NNN_<name>/{start.ss, keys.json}: one rec_segments.py run dir a stretch
                                       (needs the local states)
A packed log: gzip of JSON [[keys, count], ...] (run-length), frames = sum of the counts. Headless, muted."""
import gzip
import json
import shutil
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
W = ROOT / 'work' / 'ship'
DATA = ROOT / 'data' / 'realrun'

# (chapter, name, start state, key log, frames (None: search the end state / the whole log), end state)
F, J, S = 'fat/', 'jan/', 'sb/'
CHAIN = [
    ('ship', 'ship_c14', 'real4/start.ss', 'real4/keys.json', None, 'real4/end.ss'),
    ('ship', 'ship_c15', 'real4/end.ss', 'real4b/keys.json', None, 'real4b/c15_after_buu.ss'),
    ('fat', 'd04_bandana', 'real4b/c15_after_buu.ss', F + 'keys_d00_d04.json', None, F + 'd04_bandana.ss'),
    ('fat', 'd06_thieves', F + 'd04_bandana.ss', F + 'keys_d04_fail.json', 0, F + 'd06_thieves.ss'),
    ('fat', 'd10_map', F + 'd06_thieves.ss', F + 'keys_d06_d10.json', 0, F + 'd10_map.ss'),
    ('fat', 'd11_pilaf', F + 'd10_map.ss', F + 'keys_d10_d11.json', None, F + 'd11_pilaf.ss'),
    ('fat', 'd11b_rock', F + 'd11_pilaf.ss', F + 'keys_d11_d11b.json', None, F + 'd11b_rock.ss'),
    ('fat', 'd13_pilaf_won', F + 'd11b_rock.ss', F + 'keys_d11b_d13.json', 0, F + 'd13_pilaf_won.ss'),
    ('fat', 'd15_map', F + 'd13_pilaf_won.ss', F + 'keys_d13_fail.json', 0, F + 'd15_map.ss'),
    ('fat', 'd16_desert', F + 'd15_map.ss', F + 'keys_d15_d16.json', 0, F + 'd16_desert.ss'),
    ('fat', 'db2_desert', F + 'd16_desert.ss', F + 'db2/keys.json', None, F + 'db2/end.ss'),
    ('fat', 'd20_airship', F + 'db2/end.ss', F + 'd20_airship.keys.json', None, F + 'd20_airship.ss'),
    ('fat', 'db5_a01', F + 'd20_airship.ss', F + 'db5/keys_a00_fail.json', 0, F + 'db5/a01_12-27.ss'),
    ('fat', 'db5_a02', F + 'db5/a01_12-27.ss', F + 'db5/a02_warlord.keys.json', None, F + 'db5/a02_warlord.ss'),
    ('fat', 'db5_a04', F + 'db5/a02_warlord.ss', F + 'db5/keys_a02_warlord.json', 0, F + 'db5/a04_escape.ss'),
    ('fat', 'db5_end', F + 'db5/a04_escape.ss', F + 'db5/keys_a04_escape.json', None, F + 'db5/end.ss'),
    ('fat', 'd30_fukurou', F + 'db5/end.ss', F + 'd30_fukurou.keys.json', None, F + 'd30_fukurou.ss'),
    ('fat', 'db6_fukurou', F + 'd30_fukurou.ss', F + 'db6/keys.json', None, F + 'db6/end.ss'),
    ('fat', 'd40_pyramid', F + 'db6/end.ss', F + 'd40_pyramid.keys.json', None, F + 'd40_pyramid.ss'),
    ('fat', 'db7_p01', F + 'd40_pyramid.ss', F + 'db7/p01_challenger.keys.json', None, F + 'db7/p01_challenger.ss'),
    ('fat', 'db7_p05', F + 'db7/p01_challenger.ss', F + 'db7/keys_p01_challenger.json', 0, F + 'db7/p05_sw1.ss'),
    ('fat', 'db7_p05p', F + 'db7/p05_sw1.ss', F + 'db7/keys_p05_sw1_pre.json', None, F + 'db7/p05_sw1_punched.ss'),
    ('fat', 'db7_p08', F + 'db7/p05_sw1_punched.ss', F + 'db7/keys_p05_sw1.json', None, F + 'db7/p08_sw4.ss'),
    ('fat', 'db7_end', F + 'db7/p08_sw4.ss', F + 'db7/keys_p08_sw4.json', None, F + 'db7/end.ss'),
    ('fat', 'd41_pyramid', F + 'db7/end.ss', F + 'd41_pyramid.keys.json', None, F + 'd41_pyramid.ss'),
    ('fat', 'db7fix', F + 'd41_pyramid.ss', F + 'db7fix/keys.json', None, F + 'db7fix/end.ss'),
    ('fat', 'd50_nataday', F + 'db7fix/end.ss', F + 'd50_nataday.keys.json', None, F + 'd50_nataday.ss'),
    ('fat', 'db4_j_down', F + 'd50_nataday.ss', F + 'db4/keys_a_fail.json', 0, F + 'db4/j_down.ss'),
    ('fat', 'db4b_end', F + 'db4/j_down.ss', F + 'db4b/keys.json', None, F + 'db4b/end.ss'),
    ('fat', 'x01_city', F + 'db4b/end.ss', F + 'dbx/x01_city.keys.json', None, F + 'dbx/x01_city.ss'),
    ('fat', 'x01_map', F + 'dbx/x01_city.ss', F + 'dbx/x01_map.keys.json', None, F + 'dbx/x01_map.ss'),
    ('fat', 'x03_goku_pre', F + 'dbx/x01_map.ss', F + 'dbx/keys_x01_map.json', None, F + 'dbx/x03_goku_pre.ss'),
    ('fat', 'x03_goku', F + 'dbx/x03_goku_pre.ss', F + 'dbx/x03_goku.keys.json', None, F + 'dbx/x03_goku.ss'),
    ('fat', 'x05_s026', F + 'dbx/x03_goku.ss', F + 'dbx/keys_x03_goku.json', 0, F + 'dbx/x05_s026.ss'),
    ('fat', 'dbx_end', F + 'dbx/x05_s026.ss', F + 'dbx/keys_x05_s026.json', None, F + 'dbx/end.ss'),
    ('janemba', 'j00_yemma', F + 'dbx/end.ss', J + 'j00_yemma.keys.json', None, J + 'j00_yemma.ss'),
    ('janemba', 'j01_hfil', J + 'j00_yemma.ss', J + 'j01_hfil.keys.json', None, J + 'j01_hfil.ss'),
    ('janemba', 'j02_goku90', J + 'j01_hfil.ss', J + 'j02_goku90.keys.json', None, J + 'j02_goku90.ss'),
    ('janemba', 'j03_goku120', J + 'j02_goku90.ss', J + 'j03_goku120.keys.json', None, J + 'j03_goku120.ss'),
    ('janemba', 'h_switch', J + 'j03_goku120.ss', J + 'h_switch.keys.json', None, J + 'h_switch.ss'),
    ('janemba', 'h_maze', J + 'h_switch.ss', J + 'h_maze.keys.json', None, J + 'h_maze.ss'),
    ('janemba', 'h_maze_punched', J + 'h_maze.ss', J + 'h_maze_punched.keys.json', None, J + 'h_maze_punched.ss'),
    ('janemba', 'h_statues', J + 'h_maze_punched.ss', J + 'h_statues.keys.json', None, J + 'h_statues.ss'),
    ('janemba', 'h_vegeta', J + 'h_statues.ss', J + 'h_vegeta.keys.json', None, J + 'h_vegeta.ss'),
    ('janemba', 'h_back1', J + 'h_vegeta.ss', J + 'b_keys.json', 0, J + 'h_back1.ss'),
    ('janemba', 'h_veg0', J + 'h_back1.ss', J + 'h_veg0.keys.json', None, J + 'h_veg0.ss'),
    ('janemba', 'h_veg1', J + 'h_veg0.ss', J + 'h_veg1.keys.json', None, J + 'h_veg1.ss'),
    ('janemba', 'h_h13', J + 'h_veg1.ss', J + 'h_h13.keys.json', None, J + 'h_h13.ss'),
    ('janemba', 'j10_arena', J + 'h_h13.ss', J + 'j10_arena.keys.json', None, J + 'j10_arena.ss'),
    ('janemba', 'j_sj2', J + 'j10_arena.ss', J + 'a_keys_arena_fail.json', 0, J + 'j_sj2.ss'),
    ('janemba', 'j_arena_end', J + 'j_sj2.ss', J + 'a_keys_sj2.json', 0, J + 'j_arena_end.ss'),
    ('superbuu', 'sb_start', J + 'j_arena_end.ss', S + 'sb_start.keys.json', None, S + 'sb_start.ss'),
    ('superbuu', 'm_gotenks', S + 'sb_start.ss', S + 'sbk_start.json', 0, S + 'm_gotenks.ss'),
    ('superbuu', 'm_gotenks_punched', S + 'm_gotenks.ss', S + 'm_gotenks_punched.keys.json', None, S + 'm_gotenks_punched.ss'),
    ('superbuu', 'm_ult_gohan', S + 'm_gotenks_punched.ss', S + 'sbk_gotenks.json', 0, S + 'm_ult_gohan.ss'),
    ('superbuu', 'g01_nataday', S + 'm_ult_gohan.ss', S + 'g01_nataday.keys.json', None, S + 'g01_nataday.ss'),
    ('superbuu', 'g02_pyramid', S + 'g01_nataday.ss', S + 'g02_pyramid.keys.json', None, S + 'g02_pyramid.ss'),
    ('superbuu', 'g03_gohan141', S + 'g02_pyramid.ss', S + 'g03_gohan141.keys.json', None, S + 'g03_gohan141.ss'),
    ('superbuu', 'g04_forest', S + 'g03_gohan141.ss', S + 'g04_forest.keys.json', None, S + 'g04_forest.ss'),
    ('superbuu', 'm_gohan_vs_buu', S + 'g04_forest.ss', S + 'sbk_ult_gohan_fail.json', 0, S + 'm_gohan_vs_buu.ss'),
    ('superbuu', 'm_wasteland', S + 'm_gohan_vs_buu.ss', S + 'sbk_gohan_vs_buu.json', 0, S + 'm_wasteland.ss'),
    # (the first m_goku_vs_buu state, at 20-6's gate, was overwritten: 20,931 frames, then the grind's log)
    ('superbuu', 'g05_goku141', S + 'm_wasteland.ss', [S + 'sbk_wasteland.json:20931', S + 'g05_goku141.keys.json'], None,
     S + 'g05_goku141.ss'),
    ('superbuu', 'm_inside_buu', S + 'g05_goku141.ss', S + 'sbk_fusion_saga.json', 0, S + 'm_inside_buu.ss'),
    ('superbuu', 'i01_21-2', S + 'm_inside_buu.ss', S + 'i01_21-2.keys.json', None, S + 'i01_21-2.ss'),
    ('superbuu', 'm_kid_buu_saga', S + 'i01_21-2.ss', S + 'sbk_inside_buu_skip.json', 0, S + 'm_kid_buu_saga.ss'),
    ('superbuu', 'k01_rested', S + 'm_kid_buu_saga.ss', S + 'k01_rested.keys.json', None, S + 'k01_rested.ss'),
    ('superbuu', 'k02_pyramid', S + 'k01_rested.ss', S + 'k02_pyramid.keys.json', None, S + 'k02_pyramid.ss'),
    ('superbuu', 'k03_goku158', S + 'k02_pyramid.ss', S + 'k03_goku158.keys.json', None, S + 'k03_goku158.ss'),
    ('superbuu', 'k04_vegeta', S + 'k03_goku158.ss', S + 'k04_vegeta.keys.json', None, S + 'k04_vegeta.ss'),
    ('superbuu', 'k05_vegeta150', S + 'k04_vegeta.ss', S + 'k05_vegeta150.keys.json', None, S + 'k05_vegeta150.ss'),
    ('superbuu', 'm_kid_buu', S + 'k05_vegeta150.ss', S + 'm_kid_buu.keys.json', None, S + 'm_kid_buu.ss'),
    ('superbuu', 'm_kid_buu_punched', S + 'm_kid_buu.ss', S + 'm_kid_buu_punched.keys.json', None, S + 'm_kid_buu_punched.ss'),
    ('superbuu', 'the_end', S + 'm_kid_buu_punched.ss', S + 'sbk_kid_buu.json', None, S + 'the_end.ss'),
]


def emu(path):
    from route import start_emu
    return start_emu(str(path))


def sig(e):
    """what identifies a game state: the status + the RNG + the logic frame counter"""
    from route import status
    s = status(e)
    return json.dumps([s, e.r32(0x03000F94), e.r32(0x03001C34)])


def load_keys(spec):
    out = []
    for f in spec if isinstance(spec, list) else [spec]:
        n = None
        if ':' in f:
            f, n = f.split(':')
        j = json.loads((W / f).read_text())
        k = j if isinstance(j, list) else j['keys']
        out += k[:int(n)] if n else k
    return [x or 0 for x in out]


def rle(keys):
    out = []
    for k in keys:
        if out and out[-1][0] == k:
            out[-1][1] += 1
        else:
            out.append([k, 1])
    return out


def unrle(r):
    return [k for k, n in r for _ in range(n)]


def pack_one(i):
    ch, name, st, kf, n, en = CHAIN[i]
    keys = load_keys(kf)
    want = sig(emu(W / en))
    e = emu(W / st)
    from route import status
    s0 = status(e)
    hit = 0 if sig_eq(e, want) and not keys else None
    for f, k in enumerate(keys):
        e.set_keys(k)
        e.run(1)
        if sig_eq(e, want):
            hit = f + 1
            if n is None and hit < len(keys) - 2:
                continue        # (a whole log: its last match)
            break
    if hit is None:
        return i, name, None, f'END STATE NOT REACHED (log {len(keys)} frames)'
    DATA.mkdir(parents=True, exist_ok=True)
    with gzip.open(DATA / f'{i:03d}_{name}.keys.gz', 'wt') as fh:
        json.dump(rle(keys[:hit]), fh, separators=(',', ':'))
    return i, name, {'n': i, 'chapter': ch, 'name': name, 'frames': hit, 'start': s0, 'end': json.loads(want)[0],
                     'local_start': 'work/ship/' + st, 'local_end': 'work/ship/' + en}, 'ok'


def sig_eq(e, want):
    # the set keys differ at a mark (a run ends with keys 0): compare after the frame as it is
    return sig(e) == want


def main():
    a = sys.argv[1:]
    if a[0] == 'pack':
        jobs = int(a[1]) if len(a) > 1 else 6
        man = []
        with ProcessPoolExecutor(jobs) as ex:
            for i, name, m, msg in ex.map(pack_one, range(len(CHAIN))):
                print(f'{i:03d} {name}: {m["frames"] if m else ""} {msg}', flush=True)
                man.append(m)
        if all(man):
            (DATA / 'manifest.json').write_text(json.dumps(man, indent=0))
            print('total frames', sum(m['frames'] for m in man))
        else:
            raise SystemExit('some stretches failed: manifest not written')
    elif a[0] == 'rebuild':
        from route import status
        out = Path(a[1]).resolve()
        out.mkdir(parents=True, exist_ok=True)
        man = json.loads((DATA / 'manifest.json').read_text())
        e = emu(a[2] if len(a) > 2 else W / 'real4/start.ss')
        for m in man:
            keys = unrle(json.loads(gzip.open(DATA / f'{m["n"]:03d}_{m["name"]}.keys.gz', 'rt').read()))
            for k in keys:
                e.set_keys(k)
                e.run(1)
            ok = status(e) == m['end']
            e.save_file(str(out / f'{m["n"]:03d}_{m["name"]}.ss'))
            print(f'{m["n"]:03d} {m["name"]}: {len(keys)} frames', 'ok' if ok else f'MISMATCH {status(e)} != {m["end"]}',
                  flush=True)
            if not ok:
                raise SystemExit(1)
    elif a[0] == 'runs':
        out = Path(a[1]).resolve()
        man = json.loads((DATA / 'manifest.json').read_text())
        for m in man:
            d = out / f'{m["n"]:03d}_{m["name"]}'
            d.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / m['local_start'], d / 'start.ss')
            keys = unrle(json.loads(gzip.open(DATA / f'{m["n"]:03d}_{m["name"]}.keys.gz', 'rt').read()))
            (d / 'keys.json').write_text(json.dumps({'from': None, 'keys': keys}))
        print(len(man), 'run dirs in', out)


if __name__ == '__main__':
    main()
