"""Rip every song and sound effect of Dragon Ball Z: Buu's Fury by playing them through the game's own sound
driver in the emulator (game logic frozen, see buu_sound.Harness) and capturing the mixed GBA output.

    .venv/bin/python tools/audio/rip_buu.py [--force] [--only music|sfx] [ids...]

Outputs
  site/public/assets/buu/audio/music/NN_<slug>.mp3   intro + one loop + 1 s tail (loop points in the catalog)
  site/public/assets/buu/audio/sfx/NN_<slug>.mp3
  data/buu/audio.json                                 catalog: names, usage, loop points, triggers, evidence
Renders are cached as WAV under work/audio/cache/ (keyed by ROM + ripper version); encoding reuses them, so
re-running after tools/extract/extract_all.py wiped site/public/assets/buu only re-encodes.
See tools/audio/AUDIO.md.
"""
import json
import hashlib
import re
import shutil
import struct
import subprocess
import sys
import wave
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from buu_sound import (ROOT, ROM, PLAYER, VOICES, SONG_TABLE, SFX_TABLE, N_SONGS, N_SFX, PLAY_SONG,  # noqa: E402
                       PLAY_SFX, STOP_ALL_SFX, MIX_RATE, Harness, song_entry, sfx_entry)
from names_buu import SONGS, SFX, ARENA_MUSIC, BOSS_MUSIC, BOSS_MUSIC_TYPES, EVENTS  # noqa: E402

VERSION = 7
CACHE = ROOT / 'work' / 'audio' / 'cache'
OUT = ROOT / 'site' / 'public' / 'assets' / 'buu' / 'audio'
CATALOG = ROOT / 'data' / 'buu' / 'audio.json'
MAX_SONG_FRAMES = 60 * 60 * 12     # 12 minutes cap while searching for the loop
CONFIRM_ROWS = 48                  # row events that must repeat to accept a loop
LOOP_MARGIN = 0.25                 # loop start this many seconds after the first repeated row (encoder-delay safe)
TAIL = 1.0                         # seconds kept after the loop end
GAIN_DB = 6.0                      # uniform gain for every file (keeps music / SFX balance as in the game)
BITRATE = '128k'


def slug(s):
    return re.sub(r'[^a-z0-9]+', '_', s.lower()).strip('_')[:48]


def rom_key():
    return hashlib.sha1(ROM.read_bytes()).hexdigest()[:12] + f'_v{VERSION}'


# ------------------------------------------------------------------ area table (song usage by map)
AREAS = 0x0808E2E0
N_AREAS = 0x1C4
STRTAB = 0x0806BCE8


def ustr(rom, sid):
    p = struct.unpack_from('<I', rom, STRTAB - 0x08000000 + 4 * sid)[0] - 0x08000000
    out = []
    while True:
        c = struct.unpack_from('<H', rom, p)[0]
        if c == 0:
            break
        out.append(chr(c) if 0x20 <= c < 0x7F or 0xA0 <= c < 0x100 else '')
        p += 2
    return ''.join(out)


def area_music(rom):
    """{song id: [ 'w-m name', ...]} from the area table (+0x24 = song id, or a bytecode script pointer)."""
    use, last = {}, {}
    scripts = []
    for i in range(N_AREAS):
        a = AREAS - 0x08000000 + 0x38 * i
        world, mp = rom[a], rom[a + 1]
        name_id, song = struct.unpack_from('<I', rom, a + 0xC)[0], struct.unpack_from('<I', rom, a + 0x24)[0]
        nm = ustr(rom, name_id) if name_id else ''
        if nm:
            last[world] = nm
        label = f'{world}-{mp} ' + (nm or f'({last.get(world, "unnamed")})')
        if song >> 16:
            scripts.append((label, song))
        elif song != 0xFF:
            use.setdefault(song, []).append(label)
    return use, scripts


# ------------------------------------------------------------------ rendering
def to_np(pcm):
    return np.frombuffer(pcm, dtype=np.int16).reshape(-1, 2)


def write_wav(path, a, rate):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), 'wb') as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(np.ascontiguousarray(a, dtype=np.int16).tobytes())


def read_wav(path):
    with wave.open(str(path), 'rb') as w:
        return to_np(w.readframes(w.getnframes())), w.getframerate()


def find_loop(events):
    """events: [(sample_pos, key)] at every row change. Returns (i_start, i_repeat) or None: the first event
    whose key reappears later with the following CONFIRM_ROWS keys identical too, and the same row lengths
    (a speed change during the first pass makes the rows of pass 1 and pass 2 last differently)."""
    keys = [k for _, k in events]
    pos = [p for p, _ in events]
    tol = 600   # samples (~1 frame of capture granularity)

    def same(j, i, n):
        if keys[j:j + n] != keys[i:i + n]:
            return False
        return all(abs((pos[j + k + 1] - pos[j + k]) - (pos[i + k + 1] - pos[i + k])) <= tol for k in range(n - 1))

    first = {}
    for i, k in enumerate(keys):
        if k in first:
            for j in first[k]:
                n = min(CONFIRM_ROWS, len(keys) - i)
                if n >= CONFIRM_ROWS and same(j, i, n):
                    return j, i
            first[k].append(i)
        else:
            first[k] = [i]
    return None


def refine_period(mono, w0, period, rate, search=1500):
    """Sample-exact loop length: maximise correlation of a 2 s window at w0 with the one `period` later."""
    n = int(2.0 * rate)
    ref = mono[w0:w0 + n]
    if len(ref) < n or np.abs(ref).max() < 1:
        return period, 1.0
    best, bp = -2, period
    for d in range(-search, search + 1):
        s = w0 + period + d
        seg = mono[s:s + n]
        if len(seg) < n:
            continue
        c = float(np.dot(ref, seg) / (np.linalg.norm(ref) * np.linalg.norm(seg) + 1e-9))
        if c > best:
            best, bp = c, period + d
    return bp, best


def steady_start(mono, t1, period, rate):
    """Earliest s >= t1 from which pass 1 equals pass 2 sample for sample (notes ringing from before the loop
    point have died out). Falls back to the start of pass 2 (identical by determinism)."""
    blk = int(0.02 * rate)
    nb = period // blk
    x = mono[t1:t1 + nb * blk].reshape(nb, blk)
    y = mono[t1 + period:t1 + period + nb * blk].reshape(nb, blk)
    diff = np.sqrt(((x - y) ** 2).mean(axis=1))
    ref = np.sqrt((x ** 2).mean(axis=1))
    # the output is a sample-and-hold of the 15978 Hz mix at 32768 Hz, so one period is not a whole number of
    # output samples: allow the jitter level of the settled part of the loop
    med = float(np.median(diff[nb // 2:])) if nb > 2 else 0.0
    bad = np.nonzero(diff > np.maximum(np.maximum(64.0, 3 * med), 0.2 * ref))[0]
    if not len(bad):
        return t1, 0.0
    s = t1 + (int(bad[-1]) + 1) * blk
    if s - t1 > 0.85 * period:
        return t1 + period, float(diff.mean())
    return s, float(diff.mean())


def best_seam(mono, lo, period, rate, span=8.0):
    """Loop start in [lo, lo + span s] where the audio just before the loop end best matches the audio just before
    the loop start (sustained instrument samples keep running across the song's wrap, so pass 1 and pass 2 are
    musically but not always sample-wise identical; pick the least audible seam)."""
    w = 512
    hi = min(lo + int(span * rate), lo + period // 2)
    best, bx = None, lo
    for x in range(lo, hi, 32):
        a = mono[x - w:x + w // 4]
        b = mono[x + period - w:x + period + w // 4]
        if len(b) < len(a) or len(a) == 0:
            break
        d = float(np.sqrt(((a - b) ** 2).mean()))
        if best is None or d < best:
            best, bx = d, x
    return bx, best or 0.0


def silent_run(mono, rate, secs=3.0):
    """Start of the first run of >= secs digital silence after some sound (one-shot jingles idle on silent rows)."""
    blk = int(0.05 * rate)
    nb = len(mono) // blk
    loud = np.abs(mono[:nb * blk].reshape(nb, blk)).max(axis=1) > 0
    need = int(secs / 0.05)
    run = 0
    heard = False
    for k, l in enumerate(loud):
        if l:
            heard = True
            run = 0
        elif heard:
            run += 1
            if run >= need:
                return (k - run + 1) * blk
    return None


def render_song(h, sid):
    e = h.e
    h.silence()
    _, pcm = h.call(PLAY_SONG, PLAYER, SONG_TABLE + 20 * sid)
    chunks = [to_np(pcm)]
    total = len(chunks[0])
    events = []
    last = None
    ended = False
    loop = None
    for f in range(MAX_SONG_FRAMES):
        a = to_np(e.run_audio(1))
        chunks.append(a)
        total += len(a)
        if e.r8(PLAYER + 5) == 0:
            ended = True
            break
        # sequencer position: order index, row
        key = (e.r32(PLAYER + 0x10), e.r8(PLAYER + 1))
        if key[:2] != (last or (None, None))[:2]:
            events.append((total - len(a), key))
        last = key
        if f % 600 == 599 and loop is None:
            loop = find_loop(events)
            if loop:
                break
    if loop is None and not ended:
        loop = find_loop(events)
    rate = h.rate
    if ended:
        for _ in range(int(3 * 60)):   # let the last notes ring out
            chunks.append(to_np(e.run_audio(1)))
    if loop:
        j, i = loop
        need = events[i][0] + (events[i][0] - events[j][0]) + int((LOOP_MARGIN + TAIL + 12) * rate)
        while total < need:
            a = to_np(e.run_audio(60))
            chunks.append(a)
            total += len(a)
    a = np.concatenate(chunks)
    h.silence()
    meta = {'ended': ended, 'loop': None}
    mono = a.astype(np.float64).mean(axis=1)
    if loop:
        j, i = loop
        t1, period = events[j][0], events[i][0] - events[j][0]
        period, corr = refine_period(mono, t1 + period // 2, period, rate)
        s, resid = steady_start(mono, t1, period, rate)
        quiet = silent_run(mono[:t1 + period], rate)
        if quiet is not None:     # one-shot jingle followed by silent rows: keep it as a non-looping track
            a = a[:quiet + int(0.05 * rate)]
            meta['ended'] = True
            meta['jingle'] = True
        else:
            ls, seam = best_seam(mono, s + int(LOOP_MARGIN * rate), period, rate)
            le = ls + period
            a = a[:le + int(TAIL * rate)]
            meta['loop'] = {'start': ls / rate, 'end': le / rate, 'start_sample': ls, 'end_sample': le,
                            'correlation': round(corr, 4), 'seam_rms': round(seam, 1), 'steady_after': round((s - t1) / rate, 3),
                            'first_repeat': list(events[j][1]), 'rows_in_loop': i - j}
    else:
        nz = np.nonzero(np.abs(a).max(axis=1) > 0)[0]
        a = a[:(nz[-1] + 1 if len(nz) else 0) + int(0.05 * rate)]
    # leading silence (the first mix chunk after the call)
    nz = np.nonzero(np.abs(a).max(axis=1) > 0)[0]
    lead = int(nz[0]) if len(nz) else 0
    a = a[lead:]
    if meta['loop']:
        for k in ('start_sample', 'end_sample'):
            meta['loop'][k] -= lead
        meta['loop']['start'] = meta['loop']['start_sample'] / rate
        meta['loop']['end'] = meta['loop']['end_sample'] / rate
    meta['duration'] = len(a) / rate
    return a, meta


def render_sfx(h, i, info):
    e = h.e
    h.silence()
    _, pcm = h.call(PLAY_SFX, VOICES, SFX_TABLE + 12 * i, 0x40, 0)
    chunks = [to_np(pcm)]
    rate = h.rate
    true_len = info['length'] / info['rate']
    if info['loop']:
        want = (info['loop_start'] + 2 * (info['length'] - info['loop_start'])) / info['rate'] + 0.1
        frames = int(want * 59.7275) + 2
        chunks.append(to_np(e.run_audio(frames)))
        h.call(STOP_ALL_SFX, VOICES)
    else:
        for _ in range(int(true_len * 60) + 240):
            chunks.append(to_np(e.run_audio(1)))
            if h.busy_voices() == 0:
                break
        chunks.append(to_np(e.run_audio(2)))
    a = np.concatenate(chunks)
    h.silence()
    nz = np.nonzero(np.abs(a).max(axis=1) > 0)[0]
    if not len(nz):
        return a[:0], {'duration': 0, 'loop': None}
    # onset: first audible sample minus the PCM's own leading zeros
    rom = ROM.read_bytes()
    pcm = rom[info['pcm'] - 0x08000000: info['pcm'] - 0x08000000 + info['length']]
    lz = next((k for k, b in enumerate(pcm) if b != 0), 0)
    onset = max(0, int(nz[0]) - int(round(lz / info['rate'] * rate)))
    end = int(nz[-1]) + 1
    meta = {'loop': None}
    if info['loop']:
        ls = info['loop_start'] / info['rate']
        le = info['length'] / info['rate']
        meta['loop'] = {'start': round(ls, 5), 'end': round(le, 5)}
        end = min(end, onset + int((le + (le - ls)) * rate))
    a = a[onset:end]
    meta['duration'] = len(a) / rate
    return a, meta


# ------------------------------------------------------------------ encoding
def encode(wav, dst, gain_db):
    dst.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', str(wav), '-af', f'volume={gain_db}dB',
                    '-ar', '32000', '-c:a', 'libmp3lame', '-b:a', BITRATE, str(dst)], check=True)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    force = '--force' in sys.argv
    only = None
    if '--only' in sys.argv:
        only = sys.argv[sys.argv.index('--only') + 1]
        args = [a for a in args if a != only]
    ids = {int(a) for a in args}
    rom = ROM.read_bytes()
    key = rom_key()
    cdir = CACHE / key
    cdir.mkdir(parents=True, exist_ok=True)
    for old in CACHE.iterdir():     # renders of older ripper versions
        if old.is_dir() and old.name != key:
            shutil.rmtree(old, ignore_errors=True)
    h = None

    def harness():
        nonlocal h
        if h is None:
            h = Harness()
        return h

    songs_meta, sfx_meta = {}, {}
    for kind, n, render, store in (('music', N_SONGS, None, songs_meta), ('sfx', N_SFX, None, sfx_meta)):
        if only and only != kind:
            continue
        for i in range(n):
            wav = cdir / kind / f'{i:02d}.wav'
            mj = cdir / kind / f'{i:02d}.json'
            if not force and wav.exists() and mj.exists() and not (ids and i in ids):
                store[i] = json.loads(mj.read_text())
                continue
            if kind == 'music':
                a, meta = render_song(harness(), i)
            else:
                a, meta = render_sfx(harness(), i, sfx_entry(rom, i))
            write_wav(wav, a, harness().rate)
            meta['peak'] = int(np.abs(a.astype(np.int32)).max()) if len(a) else 0
            mj.write_text(json.dumps(meta))
            store[i] = meta
            lp = meta.get('loop')
            print(kind, i, f"{meta['duration']:.2f}s", 'loop' if lp else ('end' if meta.get('ended') else ''),
                  f"{lp['start']:.3f}-{lp['end']:.3f} corr {lp.get('correlation', '')}" if lp else '', flush=True)
    # fill in anything not re-rendered this run (for --only)
    for kind, n, store in (('music', N_SONGS, songs_meta), ('sfx', N_SFX, sfx_meta)):
        for i in range(n):
            mj = cdir / kind / f'{i:02d}.json'
            if i not in store and mj.exists():
                store[i] = json.loads(mj.read_text())
    write_outputs(rom, cdir, songs_meta, sfx_meta)


def write_outputs(rom, cdir, songs_meta, sfx_meta):
    rate = 32768
    peak = max([m.get('peak', 0) for m in list(songs_meta.values()) + list(sfx_meta.values())] + [1])
    gain = min(GAIN_DB, 20 * np.log10(32000 / peak))
    shutil.rmtree(OUT, ignore_errors=True)
    use, area_scripts = area_music(rom)
    music, sfx = [], []
    for i in sorted(songs_meta):
        m = songs_meta[i]
        n = SONGS.get(i, {})
        name = n.get('name') or f'Song {i:02d} (no play site found)'
        rel = f'music/{i:02d}_{slug(name)}.mp3'
        encode(cdir / 'music' / f'{i:02d}.wav', OUT / rel, gain)
        ent = song_entry(rom, i)
        music.append({
            'id': i, 'name': name, 'kind': 'music', 'file': f'assets/buu/audio/{rel}',
            'duration': round(m['duration'], 4),
            'loop': {'start': round(m['loop']['start'], 5), 'end': round(m['loop']['end'], 5),
                     'match': m['loop']['correlation']} if m.get('loop') else None,
            'ends': bool(m.get('ended')),
            'context': n.get('context', ''), 'confidence': n.get('confidence', 'none'),
            'areas': use.get(i, []), 'scripts': n.get('scripts', []),
            'evidence': n.get('evidence', 'area table +0x24' if use.get(i) else 'no reference found (area table, '
                              'script bytecode, native calls)'),
            'tempo': ent['tempo'], 'channels': ent['channels'], 'header': f"0x{ent['header']:08X}",
        })
    for i in sorted(sfx_meta):
        m = sfx_meta[i]
        n = SFX.get(i, {})
        info = sfx_entry(rom, i)
        name = n.get('name') or f'Sound {i:02d}'
        rel = f'sfx/{i:02d}_{slug(name)}.mp3'
        encode(cdir / 'sfx' / f'{i:02d}.wav', OUT / rel, gain)
        sfx.append({
            'id': i, 'name': name, 'kind': 'sfx', 'file': f'assets/buu/audio/{rel}',
            'duration': round(m['duration'], 4), 'loop': m.get('loop'),
            'trigger': n.get('trigger', ''), 'confidence': n.get('confidence', 'none'),
            'game_volume': n.get('volume', 64), 'evidence': n.get('evidence', ''),
            'sample': {'rom': f"0x{info['pcm']:08X}", 'rate': info['rate'], 'length': info['length'],
                       'loop': info['loop'], 'loop_start': info['loop_start']},
        })
    doc = {
        'game': 'buu', 'generated_by': 'tools/audio/rip_buu.py', 'format_doc': 'tools/audio/AUDIO.md',
        'engine': {
            'name': 'Webfoot custom tracker driver (not MP2K)',
            'mix_rate': round(MIX_RATE, 1), 'output': 'stereo, DMA A = left, DMA B = right, 8-bit',
            'song_table': f'0x{SONG_TABLE:08X}', 'sfx_table': f'0x{SFX_TABLE:08X}',
            'play_song': '0x0803F110', 'play_sfx': '0x0803E378', 'stop_sfx': '0x0803E302', 'stop_all_sfx': '0x0803E2EC',
            'sfx_voices': 16, 'sfx_policy': 'first free voice of 16, otherwise the request is dropped; no priority, '
                                             'no per-sound limit; volume 0..64, pan s8',
        },
        'render': {'sample_rate': 32000, 'source_rate': rate, 'gain_db': round(float(gain), 2), 'codec': f'mp3 {BITRATE}',
                   'loop_note': 'loop.start/loop.end in seconds of the file; the loop is periodic from start, so '
                                'jumping end->start is seamless even with a few ms of decoder offset'},
        'arena_music': ARENA_MUSIC, 'boss_music': BOSS_MUSIC, 'boss_music_types': BOSS_MUSIC_TYPES, 'events': EVENTS,
        'area_scripts': [{'areas': l, 'script': f'0x{s:08X}'} for l, s in area_scripts],
        'music': music, 'sfx': sfx,
    }
    CATALOG.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + '\n')
    print('wrote', CATALOG.relative_to(ROOT), len(music), 'songs', len(sfx), 'sfx', f'gain {gain:.1f} dB')


if __name__ == '__main__':
    main()
