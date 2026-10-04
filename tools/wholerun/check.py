"""check.py [--chain run1|run2] [--lag none|rec] [--known FILE]: "is it still exact?" - every real run (chains.py:
run1 = the first playthrough, power-on to the credits, 1.67M rows; run2 = the second, played differently) through the
app in no-lag mode, compared per logic iteration with the emulator's recording: RNG, area, position, script depth,
story flags and the sound requests. Exit 0 = exact everywhere outside each chain's known ranges (its
known_stops.json); 1 = a new stop (printed: chain, row, stretch, what); 2 = setup missing.

Legs, all run in parallel: per chain, the app from power-on (no import) up to the first known range's first row,
then from the game's state cut right after each known range up to the next one / the end. A cut is made once
(<chain dir>/cuts/c<ROW>.json via resync.py, ~10 min of emulator replay) and reused. Headless, muted. ~30 s once
built. Needs each chain's master (tools/wholerun/record.py [--chain run2]) and work/wholerun/power_on.json; a
chain whose master is missing is reported and skipped (run1's: tools/wholerun/board.py)."""
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'wholerun'))
import chains  # noqa: E402

W = chains.W
a = sys.argv[1:]
lag = a[a.index('--lag') + 1] if '--lag' in a else 'none'
picked = [chains.chain_arg(a)] if '--chain' in a else list(chains.NAMES)
power_on = W / 'power_on.json'
if not power_on.exists():
    print(f'missing {power_on} - run tools/wholerun/board.py first')
    sys.exit(2)

tgt = ROOT / 'engine' / 'target-wholerun'
b = subprocess.run(['cargo', 'build', '--release', '--target-dir', str(tgt), '-p', 'wf-app', '--example', 'wholerun'],
                   cwd=ROOT / 'engine', capture_output=True, text=True)
if b.returncode:
    print(b.stderr[-3000:])
    sys.exit(2)
exe = tgt / 'release' / 'examples' / 'wholerun'

legs, summary, missing = [], {}, []
for chain in picked:
    P = chains.paths(chain)
    if not P['master'].exists():
        missing.append(chain)
        continue
    rows = json.loads((P['master'].parent / (P['master'].name + '.index.json')).read_text())['rows']
    kf = Path(a[a.index('--known') + 1]) if '--known' in a else P['known']
    known = sorted(json.loads(kf.read_text())['stops'], key=lambda k: k['from']) if kf.exists() else []
    summary[chain] = (rows, known)
    start = 0
    for k in known:
        legs.append((chain, start, k['from']))
        start = k['to'] + 1
    legs.append((chain, start, rows))
for chain, s, _ in legs:
    cut = chains.paths(chain)['cuts'] / f'c{s}.json'
    if s and not cut.exists():
        print(f'{chain}: making the cut at row {s} (once, ~10 min)', flush=True)
        subprocess.run([sys.executable, str(ROOT / 'tools' / 'wholerun' / 'resync.py'), '--chain', chain, '--lag', lag,
                        '--from', str(s), '--max-stops', '1', '--log', str(cut.parent / 'check_cut.jsonl')], cwd=ROOT,
                       capture_output=True)
        if not cut.exists():
            print(f'{chain}: could not make the cut at row {s}')
            sys.exit(2)


def run(leg):
    chain, s, e = leg
    P = chains.paths(chain)
    rep = P['dir'] / f'check_{lag}_{s}.json'
    cmd = [str(exe), str(P['master']), '--lag', lag, '--frames', str(e), '--report', str(rep)]
    if lag == 'none' and P['sounds'].exists():
        cmd += ['--sounds', str(P['sounds'])]
    cmd += ['--import', str(P['cuts'] / f'c{s}.json'), '--from-row', str(s)] if s else ['--power-on', str(power_on)]
    if rep.exists():
        rep.unlink()
    p = subprocess.run(cmd, cwd=ROOT / 'engine', capture_output=True, text=True)
    if p.returncode or not rep.exists():
        return leg, None, (p.stdout + p.stderr)[-2000:]
    return leg, json.loads(rep.read_text()), ''


bad = 0
with ThreadPoolExecutor(max(1, len(legs))) as ex:
    for (chain, s, e), r, err in ex.map(run, legs):
        name = f'{chain} ' + ('power-on' if s == 0 else f'cut {s:,}')
        if r is None:
            print(f'FAIL {name} .. {e:,}: the app did not finish\n{err}')
            bad += 1
            continue
        probs = []
        if r['exact_to'] < e:
            st = r['exact_stretch']
            probs.append(f"stop at row {r['exact_to']:,} ({st}, chapter {chains.chapter_of(chain, st)}): {r['first']}")
        sf = r.get('sounds_first')
        if sf:
            probs.append(f"sound at row {sf['row']:,} ({sf['stretch']}): {sf['what'][:160]} ({r['sounds_bad']} iterations)")
        if r['script_errors']:
            probs.append(f"{r['script_errors']} script errors")
        # a chain with an expected.json (a ratchet: run 2 while its stops are being fixed) passes when its first
        # difference (stop or sound) is no earlier than the recorded one; later = an improvement, the file is bumped
        exp_p = chains.paths(chain)['expected']
        if probs and s == 0 and exp_p.exists() and lag == 'none':
            exp = json.loads(exp_p.read_text())
            first = min(r['exact_to'], sf['row'] if sf else e)
            if r['script_errors']:
                pass
            elif first < exp['first_diff']:
                probs.insert(0, f"WORSE than expected (first difference row {first:,} < {exp['first_diff']:,})")
            else:
                if first > exp['first_diff']:
                    print(f"IMPROVED {name}: first difference row {exp['first_diff']:,} -> {first:,} "
                          f"({exp_p.relative_to(ROOT)} bumped)")
                    exp_p.write_text(json.dumps({**exp, 'first_diff': first, 'stop': r['first'],
                                                 'stretch': r['exact_stretch']}, indent=1) + '\n')
                print(f"ok   {name} .. {e:,}: as expected, first difference row {first:,} - " + ' | '.join(probs))
                continue
        if probs:
            bad += 1
            print(f'FAIL {name} .. {e:,}: ' + ' | '.join(probs))
        else:
            snd = f", sounds {r['sounds_iters']:,} iterations" if r.get('sounds_compared') else ''
            print(f'ok   {name} .. {e:,}: exact ({r["stepped"]:,} logic iterations{snd})')
for chain, (rows, known) in summary.items():
    skipped = ', '.join(f"{k['from']:,}..{k['to']:,} ({k['stretch']})" for k in known)
    print(f'{chain}: {rows:,} rows; known ranges skipped: {skipped or "none"}')
for chain in missing:
    print(f'{chain}: no recording ({chains.paths(chain)["master"]}) - skipped; tools/wholerun/record.py --chain {chain}')
print(('NEW STOP(S)' if bad else 'EXACT') + f' - {lag} lag')
sys.exit(1 if bad else 0)
