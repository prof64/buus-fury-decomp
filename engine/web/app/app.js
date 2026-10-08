// Browser shell: ROM drop -> verify -> extract in a worker -> cache (OPFS / IndexedDB) -> run the game wasm
// (render + input on the main thread, the sound driver in an AudioWorklet). See engine/docs/frontend.md.
import { openStore } from './store.js';
import { tar, untar, CATEGORIES } from './tar.js';
import { startNet } from './net.js';
import { linkConfig, startLink } from './link.js';

const EXTRACTOR_URL = new URL('../wf_wasm.wasm', import.meta.url).href;
const GAME_URL = new URL('wf_web.wasm', import.meta.url).href;
const BUU_SHA1 = 'e65738e9d67688309f09811a54f495523ec9aada';
const $ = (id) => document.getElementById(id);
const qs = new URLSearchParams(location.search);
const T0 = performance.now();
// muted: the sound driver still runs in the worklet (events logged in window.wfAudioLog) but outputs silence.
// Default for automation (navigator.webdriver, e.g. headless test browsers) unless ?sound is given.
const MUTED = qs.has('mute') || (navigator.webdriver && !qs.has('sound'));
window.wfAudioLog = [];
const timings = {};
window.wfTimings = timings;

function show(id) {
  for (const s of document.querySelectorAll('.screen')) s.classList.toggle('on', s.id === id);
}
const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
const ascii = (b) => String.fromCharCode(...b).replace(/\0+$/, '');
const fetchBytes = async (u) => new Uint8Array(await (await fetch(u)).arrayBuffer());
const enc = new TextEncoder();
const dec = new TextDecoder();

const store = await openStore();
const readMeta = async () => {
  const b = await store.get('meta.json');
  return b ? JSON.parse(dec.decode(b)) : {};
};
const writeMeta = (m) => store.put('meta.json', enc.encode(JSON.stringify(m)));
// launcher settings (wf_app::launcher::Settings) and the story save slots (hex) live in localStorage
const lsGet = (k) => {
  try {
    return JSON.parse(localStorage.getItem(k));
  } catch {
    return null;
  }
};
const lsSet = (k, v) => {
  try {
    localStorage.setItem(k, typeof v === 'string' ? v : JSON.stringify(v));
  } catch {}
};
// the 3 game slots, then the options block (wf_app::launcher::saves_of / options_of: text speed, volumes) - without
// it a reload started every game with the default options
const readSlots = () => [0, 1, 2, 3].map((i) => localStorage.getItem(`wf.save.${i}`) || '');


// ------------------------------------------------------------------ ROM check (friendly errors)

async function checkRom(rom) {
  const sha = hex(await crypto.subtle.digest('SHA-1', rom));
  if (sha === BUU_SHA1) return null;
  if (rom.length < 0xC0 || rom[0xB2] !== 0x96)
    return `That file is not a Game Boy Advance ROM.`;
  const title = ascii(rom.subarray(0xA0, 0xAC));
  const code = ascii(rom.subarray(0xAC, 0xB0));
  if (code.startsWith('BG3')) {
    if (code[3] === 'P') return `This is the European release of Buu's Fury (${code}). The engine needs the USA release (BG3E).`;
    if (code[3] === 'E')
      return `This is Buu's Fury (USA), but not the known good dump:\nSHA-1 ${sha}\nexpected ${BUU_SHA1}\nIt may be patched, trimmed or corrupted.`;
    return `This is a different region of Buu's Fury (${code}). The engine needs the USA release (BG3E).`;
  }
  return `That ROM is "${title}" (${code}), not Dragon Ball Z: Buu's Fury.`;
}

// ------------------------------------------------------------------ extraction

function extract(rom, extractorId) {
  show('progress');
  return new Promise((resolve, reject) => {
    const w = new Worker(new URL('extract-worker.js', import.meta.url), { type: 'module' });
    w.onmessage = async (e) => {
      const m = e.data;
      if (m.type === 'progress') {
        $('pbar').style.width = `${(100 * m.done) / m.total}%`;
        $('ptext').textContent = `${m.category} (${m.done + 1}/${m.total}) · ${(m.ms / 1000).toFixed(1)} s`;
      } else if (m.type === 'done') {
        $('pbar').style.width = '100%';
        timings.extract_ms = Math.round(m.ms);
        $('ptext').textContent = `${m.files} files, ${(m.bytes / 1e6).toFixed(1)} MB in ${(m.ms / 1000).toFixed(1)} s · saving…`;
        w.terminate();
        const t = performance.now();
        await store.put('content/base.tar', m.tar);
        const meta = await readMeta();
        meta.extractor = extractorId;
        meta.extracted_at = new Date().toISOString();
        await writeMeta(meta);
        timings.save_ms = Math.round(performance.now() - t);
        resolve(m.tar);
      } else if (m.type === 'error') {
        w.terminate();
        reject(new Error(m.message));
      }
    };
    w.onerror = (e) => reject(new Error(e.message || 'extraction worker failed'));
    w.postMessage({ rom, wasmUrl: EXTRACTOR_URL });
  });
}

// ------------------------------------------------------------------ boot

const extractorBytes = await fetchBytes(EXTRACTOR_URL);
// cache key: the extractor binary + the category list the worker extracts
const extractorId = hex(await crypto.subtle.digest('SHA-256', new Uint8Array([...extractorBytes, ...enc.encode(CATEGORIES.join(','))])));
const meta = await readMeta();
let baseTar = null;
if (meta.extractor === extractorId) baseTar = await store.get('content/base.tar');
if (!baseTar) {
  const rom = await store.get('rom.gba');
  if (rom && !(await checkRom(rom))) {
    $('ptitle').textContent = 'Engine updated: re-extracting game content…';
    baseTar = await extract(rom, extractorId).catch((e) => {
      console.error(e);
      return null;
    });
  }
}
if (!baseTar) baseTar = await romScreen();
timings.content_ready_ms = Math.round(performance.now() - T0);
await boot(baseTar);

function romScreen() {
  show('rom');
  return new Promise((resolve) => {
    const drop = $('drop');
    const take = async (file) => {
      $('romerr').textContent = '';
      if (!file) return;
      const rom = new Uint8Array(await file.arrayBuffer());
      const err = await checkRom(rom);
      if (err) {
        $('romerr').textContent = err;
        return;
      }
      await store.put('rom.gba', rom); // kept locally so engine updates can re-extract without asking again
      try {
        resolve(await extract(rom.slice(), extractorId));
      } catch (e) {
        show('rom');
        $('romerr').textContent = `Extraction failed: ${e.message}`;
      }
    };
    drop.onclick = () => $('pick').click();
    $('pick').onchange = (e) => take(e.target.files[0]);
    drop.ondragover = (e) => {
      e.preventDefault();
      drop.classList.add('over');
    };
    drop.ondragleave = () => drop.classList.remove('over');
    drop.ondrop = (e) => {
      e.preventDefault();
      drop.classList.remove('over');
      take(e.dataTransfer.files[0]);
    };
    window.wfDropRom = take; // automation hook (tests)
    // dev / test hook: ?devrom=<same-origin url> fetches and drops a ROM (the dev server can serve a local file)
    const dev = qs.get('devrom');
    if (dev && new URL(dev, location.href).origin === location.origin)
      fetch(dev).then((r) => r.arrayBuffer()).then((b) => take(new File([b], 'rom.gba')));
  });
}

// a picked mod folder -> mods/<id>.tar in the store (Settings panel and the launcher's Mods page)
async function installMod(files) {
  if (!files.length) return;
  const root = files[0].webkitRelativePath.split('/')[0];
  const map = new Map();
  for (const f of files) map.set(f.webkitRelativePath.slice(root.length + 1), new Uint8Array(await f.arrayBuffer()));
  if (!map.has('pack.json')) map.set('pack.json', enc.encode(JSON.stringify({ id: root, name: root, format_version: 1, depends: ['base'] })));
  if (!map.has('files.json')) map.set('files.json', enc.encode(JSON.stringify([...map.keys()])));
  const id = JSON.parse(dec.decode(map.get('pack.json'))).id || root;
  await store.put(`mods/${id}.tar`, tar(map));
  const mm = await readMeta();
  mm.mods = [...new Set([...(mm.mods || []), id])];
  await writeMeta(mm);
  location.reload();
}

async function loadMods() {
  const m = await readMeta();
  const out = [];
  const off = lsGet('wf.launcher')?.disabled_mods || [];
  for (const name of (m.mods || []).filter((n) => !off.includes(n))) {
    const t = await store.get(`mods/${name}.tar`);
    if (t) out.push({ name, files: untar(t) });
  }
  return out;
}

async function boot(baseTarBytes) {
  const t0 = performance.now();
  const base = untar(baseTarBytes);
  const mods = await loadMods();
  const packs = [base, ...mods.map((m) => m.files)];
  timings.untar_ms = Math.round(performance.now() - t0);

  // game instance
  let mem = null;
  const path = (p, n) => dec.decode(new Uint8Array(mem.buffer, p, n));
  const host = {
    pack_count: () => packs.length,
    file_size: (pk, p, n) => {
      const f = packs[pk]?.get(path(p, n));
      return f ? f.length : -1;
    },
    file_read: (pk, p, n, dst) => {
      const f = packs[pk].get(path(p, n));
      new Uint8Array(mem.buffer, dst, f.length).set(f);
    },
    log: (p, n) => console.warn('[wf]', path(p, n)),
    now_ms: () => performance.now(),
  };
  const gameBytes = await fetchBytes(GAME_URL);
  const module = await WebAssembly.compile(gameBytes);
  const inst = await WebAssembly.instantiate(module, { host });
  const x = inst.exports;
  mem = x.memory;
  const str = () => dec.decode(new Uint8Array(mem.buffer, x.wf_str_ptr(), x.wf_str_len()));
  const withStr = (s, f) => {
    const b = enc.encode(s);
    const p = x.wf_alloc(b.length);
    new Uint8Array(mem.buffer, p, b.length).set(b);
    const r = f(p, b.length);
    x.wf_dealloc(p, b.length);
    return r;
  };
  // the launcher (wf_app::launcher, drawn by the wasm in the game's font) unless the URL already picks a mode
  const MODE_QS = ['story', 'demo', 'area', 'arena', 'enemy', 'fighter', 'net', 'autostart', 'nolauncher', 'link'];
  let launch = null;
  // ?link=host:ROOM / join:ROOM: the title with link play (Multiplayer -> Dojo / Trading Post), link.js
  if (qs.has('link')) launch = { kind: 'title' };
  if (!MODE_QS.some((k) => qs.has(k))) {
    launch = await runLauncher();
    if (!launch) return; // went to the online lobby (reload)
  }
  async function runLauncher() {
    const all = (await readMeta()).mods || [];
    const settings = lsGet('wf.launcher') || {};
    const off = settings.disabled_mods || [];
    const init = { settings, slots: readSlots(), mods: all.map((name) => ({ name, enabled: !off.includes(name) })), bindings: lsGet('wf.bindings') };
    // back from a battle's end screen ("Change fighter"): open on the Battle page
    const screen = sessionStorage.getItem('wf.launcher.screen');
    if (screen) {
      init.screen = screen;
      sessionStorage.removeItem('wf.launcher.screen');
    }
    if (withStr(JSON.stringify(init), (p, n) => x.wf_launcher_init(p, n)) !== 0) return { error: str() };
    $('stage').hidden = false;
    show('');
    const canvas = $('screen');
    const g = canvas.getContext('2d', { alpha: false });
    canvas.width = 240;
    canvas.height = 160;
    window.wfLauncher = true;
    return new Promise((resolve) => {
      let done = false;
      let prevPad = 0;
      const PAD = [[12, 'ArrowUp'], [13, 'ArrowDown'], [14, 'ArrowLeft'], [15, 'ArrowRight'], [0, 'Enter'], [9, 'Enter'], [1, 'Escape'], [4, 'PageUp'], [5, 'PageDown']];
      const key = (code) => {
        if (done || !withStr(code, (p, n) => x.wf_launcher_key(p, n))) return;
        const out = JSON.parse(str());
        lsSet('wf.launcher', out.settings);
        lsSet('wf.bindings', out.bindings);
        const pr = lsGet('wf.prefs') || {};
        pr.mode = out.settings.view;
        pr.vol = out.settings.volume / 100;
        pr.pacing = !!out.settings.original_pacing;
        pr.autozoom_max = !!out.settings.auto_zoom_max;
        lsSet('wf.prefs', pr);
        const c = out.choice;
        if (c.kind === 'add_mod') {
          $('modpick').onchange = (e) => installMod([...e.target.files]);
          $('modpick').click();
        } else if (c.kind === 'online') {
          location.search = '?net';
        } else if (c.kind === 'story' || c.kind === 'title' || c.kind === 'battle' || c.kind === 'replays') {
          done = true;
          removeEventListener('keydown', onKey);
          x.wf_launcher_close();
          window.wfLauncher = false;
          resolve(c.kind === 'replays' ? { kind: 'battle', config: out.battle, replays: true } : c);
        }
        window.wfLauncherLast = out; // automation hook (tests)
      };
      const onKey = (e) => {
        if (e.repeat && !e.code.startsWith('Arrow')) return;
        e.preventDefault();
        key(e.code);
      };
      addEventListener('keydown', onKey);
      window.wfLauncherKey = key; // automation hook (tests)
      const tick = () => {
        if (done) return;
        let bits = 0;
        for (const p of navigator.getGamepads ? navigator.getGamepads() : []) {
          if (p && p.connected) p.buttons.forEach((b, i) => b.pressed && i < 32 && (bits |= 1 << i));
        }
        for (const [i, code] of PAD) if (bits & ~prevPad & (1 << i)) key(code);
        prevPad = bits;
        const dpr = window.devicePixelRatio || 1;
        const s = x.wf_launcher_frame(Math.floor(innerWidth * dpr), Math.floor(innerHeight * dpr));
        if (done) return;
        g.putImageData(new ImageData(new Uint8ClampedArray(mem.buffer, x.wf_launcher_fb_ptr(), 240 * 160 * 4).slice(), 240, 160), 0, 0);
        canvas.style.width = `${(240 * s) / dpr}px`;
        canvas.style.height = `${(160 * s) / dpr}px`;
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
  }
  if (launch?.error) {
    show('rom');
    $('romerr').textContent = `Could not start the launcher: ${launch.error}`;
    return;
  }
  // mods toggled in the launcher: rebuild the pack list the game reads
  if (launch) packs.splice(1, packs.length - 1, ...(await loadMods()).map((m) => m.files));

  let bindings = null;
  try {
    bindings = JSON.parse(localStorage.getItem('wf.bindings'));
  } catch {}
  let hotkeys = null;
  try {
    hotkeys = JSON.parse(localStorage.getItem('wf.hotkeys'));
  } catch {}
  const cfg = { players: Number(qs.get('players') || 1), bindings, hotkeys, now: Date.now() / 1000 };
  // the launcher's pick: story (new game / a save slot) or a battle config; the save slots go along
  if (launch) Object.assign(cfg, { launch, slots: readSlots() });
  const linkCfg = linkConfig(qs);
  if (linkCfg) cfg.link = linkCfg;
  // real battle by default (?arena=forest&enemy=t68&count=1&fighter=goku&seed=1); ?demo or ?area=8-3 = demo world
  if (qs.has('demo') || qs.has('area')) Object.assign(cfg, { demo: true, area: qs.get('area') || '8-3' });
  // ?story or ?story=W-A: story mode (new game, field + the game's scripts)
  if (qs.has('story')) cfg.story = qs.get('story') || '1-1';
  for (const k of ['arena', 'enemy', 'fighter']) if (qs.get(k)) cfg[k] = qs.get(k);
  for (const k of ['count', 'form', 'level', 'seed']) if (qs.get(k)) cfg[k] = Number(qs.get(k));
  const t1 = performance.now();
  // online (?net, net.js): lobby first; the match then runs as the app's simulation
  let net = null;
  if (qs.has('net')) {
    // the in-game lobby menu draws on the game canvas before the match exists
    const showStage = () => {
      $('stage').hidden = false;
      show('');
    };
    net = await startNet({ x, mem: () => mem, withStr, str, canvas: $('screen'), showStage }, qs);
    if (!net) return;
    cfg.net = true;
  }
  if (withStr(JSON.stringify(cfg), (p, n) => x.wf_app_init(p, n)) !== 0) {
    show('rom');
    $('romerr').textContent = `Could not start: ${str()}`;
    return;
  }
  timings.init_ms = Math.round(performance.now() - t1);
  const blob = () => new Uint8Array(mem.buffer, x.wf_blob_ptr(), x.wf_blob_len()).slice();
  const withBytes = (b, f) => {
    const p = x.wf_alloc(b.length);
    new Uint8Array(mem.buffer, p, b.length).set(b);
    const r = f(p, b.length);
    x.wf_dealloc(p, b.length);
    return r;
  };
  const replays = replaysUi();
  // link play's per-frame pump (disk mirror, room connection, packets): link.js
  const link = linkCfg ? startLink({ x, mem: () => mem, withStr, str }) : null;
  if (launch?.replays) setTimeout(() => replays.show(), 0);

  // settings that live in the browser
  const prefs = (() => {
    try {
      return JSON.parse(localStorage.getItem('wf.prefs')) || {};
    } catch {
      return {};
    }
  })();
  const savePrefs = () => {
    try {
      localStorage.setItem('wf.prefs', JSON.stringify(prefs));
    } catch {}
  };
  const applyMode = () => {
    const [k, z] = (prefs.mode || 'gba:0').split(':');
    x.wf_app_set_mode(k === 'gba' ? 1 : 0, Number(z));
    $('mode').value = prefs.mode || 'gba:0';
  };
  applyMode();
  // launcher Settings "Original pacing" (story mode; presentation only)
  x.wf_app_set_pacing?.(prefs.pacing ? 1 : 0);
  // launcher Settings "Auto zoom: steady / max" (PC wide auto)
  x.wf_app_set_auto_zoom?.(prefs.autozoom_max ? 1 : 0);
  if (qs.get('mode')) {
    prefs.mode = qs.get('mode');
    applyMode();
  }

  // audio (needs a user gesture)
  let audio = null;
  const queued = [];
  async function startAudio() {
    const ctx = new AudioContext({ latencyHint: 'interactive' });
    // the worklet only receives messages while the render thread runs: resume first
    ctx.resume();
    timings.audio = 'loading';
    await ctx.audioWorklet.addModule(new URL('audio-worklet.js', import.meta.url));
    timings.audio = `module ${ctx.state}`;
    const node = new AudioWorkletNode(ctx, 'wf-audio', { numberOfInputs: 0, outputChannelCount: [2] });
    node.connect(ctx.destination);
    const audioPacks = packs.map((p) => [...p.entries()].filter(([k]) => k === 'pack.json' || k.startsWith('audio/')));
    const ready = new Promise((res, rej) => {
      node.port.onmessage = (e) => (e.data.type === 'ready' ? res(e.data) : e.data.type === 'error' ? rej(new Error(e.data.message)) : (timings.audio = `${e.data.stage} ${ctx.state}`));
    });
    node.port.postMessage({ type: 'init', wasm: gameBytes, packs: audioPacks, gain: MUTED ? 0 : prefs.vol ?? 1, linear: prefs.resample !== 'hold' });
    node.onprocessorerror = (e) => (timings.audio = `processor error ${e.message || ''}`);
    await ready;
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 2048;
    node.connect(analyser);
    audio = { ctx, node, analyser, buf: new Float32Array(2048) };
    timings.audio = `${ctx.state} ${ctx.sampleRate} Hz${MUTED ? ' (muted)' : ''}`;
  }

  // stage + loop
  $('stage').hidden = false;
  show(qs.has('autostart') || net || launch ? '' : 'start');
  const canvas = $('screen');
  const g = canvas.getContext('2d', { alpha: false });
  let running = false;
  let frames = 0;
  let fpsT = performance.now();
  let fps = 0;
  let statsOn = qs.has('stats');
  let savesGen = 0;
  $('stats').style.display = statsOn ? 'block' : 'none';

  const start = async () => {
    if (running) return;
    running = true;
    show('');
    $('hudbtn').style.display = 'block';
    $('topbar').style.display = 'flex';
    document.body.classList.add('withbar');
    // the game starts when the sound driver is ready (so the music starts on frame 0 like the game), but never
    // waits more than 2 s for it; events emitted before audio is up are queued and flushed on ready
    const ta = performance.now();
    const ap = startAudio().then(
      () => {
        timings.audio_init_ms = Math.round(performance.now() - ta);
        if (queued.length) audio.node.port.postMessage({ type: 'events', words: Int32Array.from(queued.splice(0)) });
      },
      (e) => {
        timings.audio = `error: ${e.message}`;
        console.error('audio', e);
      },
    );
    await Promise.race([ap, new Promise((r) => setTimeout(r, 2000))]);
    requestAnimationFrame(frame);
    // dev / test hook: ?devinput=ArrowRight:60,ArrowDown:40 holds keys for N frames in sequence
    const script = (qs.get('devinput') || '').split(',').filter(Boolean);
    (async () => {
      for (const item of script) {
        const [code, n] = item.split(':');
        withStr(code, (p, l) => x.wf_app_key(p, l, 1, 0));
        await new Promise((r) => setTimeout(r, (Number(n) * 1000) / 59.7275));
        withStr(code, (p, l) => x.wf_app_key(p, l, 0, 0));
      }
    })();
  };
  $('start').onclick = start;
  if (qs.has('autostart') || net || launch) start();

  function pollPads() {
    const pads = navigator.getGamepads ? navigator.getGamepads() : [];
    let slot = 0;
    for (const p of pads) {
      if (!p || !p.connected) continue;
      let bits = 0;
      p.buttons.forEach((b, i) => {
        if (b.pressed && i < 32) bits |= 1 << i;
      });
      x.wf_app_pad(slot, bits >>> 0, p.axes[0] || 0, p.axes[1] || 0);
      slot++;
    }
  }

  let drawnSize = '';
  function frame() {
    pollPads();
    const dpr = window.devicePixelRatio || 1;
    const W = Math.floor(innerWidth * dpr);
    // (the button bar's height comes off the picture: the game never draws under the buttons)
    const barH = document.body.classList.contains('withbar') ? 34 : 0; // (= #topbar's CSS height; no layout read per frame)
    const H = Math.floor((innerHeight - barH) * dpr);
    x.wf_app_set_clock(Date.now() / 1000);
    // bit 30: a new picture was rendered (none when no step ran and nothing changed - a > 60 Hz display)
    const drawn = (x.wf_app_frame(W, H, net ? net.slot() : 0) & (1 << 30)) !== 0;
    if (link) link.pump();
    if (net) {
      net.pump();
      net.draw();
    }
    replays.pump();
    const w = x.wf_fb_w();
    const h = x.wf_fb_h();
    const s = x.wf_fb_scale();
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
      drawnSize = '';
    }
    const css = `${(w * s) / dpr}px ${(h * s) / dpr}px`;
    if (css !== drawnSize) {
      // (style writes only on a change: each one dirties the layout)
      drawnSize = css;
      canvas.style.width = `${(w * s) / dpr}px`;
      canvas.style.height = `${(h * s) / dpr}px`;
    }
    if (drawn) g.putImageData(new ImageData(new Uint8ClampedArray(mem.buffer, x.wf_fb_ptr(), w * h * 4), w, h), 0, 0);
    const n = x.wf_app_audio_len();
    if (n) {
      const words = new Int32Array(mem.buffer, x.wf_app_audio_ptr(), n).slice();
      for (let i = 0; i + 3 < words.length; i += 4) window.wfAudioLog.push([x.wf_app_frame_no ? x.wf_app_frame_no() : 0, ...words.subarray(i, i + 4)]);
      // (a debugging / test log: the last 20,000 events, not the whole session's)
      if (window.wfAudioLog.length > 24000) window.wfAudioLog.splice(0, window.wfAudioLog.length - 20000);
      x.wf_app_audio_clear();
      if (audio) audio.node.port.postMessage({ type: 'events', words });
      else queued.push(...words);
    }
    frames++;
    const now = performance.now();
    if (now - fpsT >= 500) {
      fps = (frames * 1000) / (now - fpsT);
      frames = 0;
      fpsT = now;
      // story mode: persist the save slots when the game saved
      const gen = x.wf_app_saves();
      if (gen !== -1 && gen !== 0xffffffff && gen !== savesGen) {
        savesGen = gen;
        JSON.parse(str()).forEach((h, i) => (h ? lsSet(`wf.save.${i}`, h) : localStorage.removeItem(`wf.save.${i}`)));
      }
      x.wf_app_stats();
      const st = JSON.parse(str());
      if (audio) {
        audio.analyser.getFloatTimeDomainData(audio.buf);
        let e = 0;
        for (const v of audio.buf) e += v * v;
        timings.audio_rms = Math.sqrt(e / audio.buf.length).toFixed(4);
      }
      window.wfStats = { fps, ...st, timings };
      if (statsOn)
        $('stats').textContent =
          `${fps.toFixed(1)} fps  frame ${st.frame}\nstep ${st.step_ms.toFixed(3)} ms  render ${st.render_ms.toFixed(2)} ms\n` +
          `view ${st.view[0]}x${st.view[1]} @${st.scale}x  hash ${st.state_hash}\n` +
          Object.entries(timings).map(([k, v]) => `${k} ${v}`).join('  ');
    }
    requestAnimationFrame(frame);
  }

  // input
  let rebinding = null;
  let rebindingHot = null;
  const MODKEYS = ['ShiftLeft', 'ShiftRight', 'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight'];
  addEventListener('keydown', (e) => {
    if (rebindingHot) {
      e.preventDefault();
      if (MODKEYS.includes(e.code)) return;
      const act = rebindingHot;
      rebindingHot = null;
      if (e.code !== 'Escape') {
        const combo = `${e.ctrlKey || e.metaKey ? 'Ctrl+' : ''}${e.altKey ? 'Alt+' : ''}${e.shiftKey ? 'Shift+' : ''}${e.code}`;
        withStr(act, (ap, al) => withStr(combo, (cp, cl) => x.wf_app_rebind_hotkey(ap, al, cp, cl)));
      }
      saveHotkeys();
      renderHotkeys();
      return;
    }
    if (rebinding) {
      e.preventDefault();
      const btn = rebinding;
      rebinding = null;
      if (e.code !== 'Escape') withStr(btn, (bp, bl) => withStr(e.code, (cp, cl) => x.wf_app_rebind(bp, bl, cp, cl)));
      saveBindings();
      renderBinds();
      return;
    }
    // Shift+Esc: back to the launcher (the recording is saved first)
    if (e.code === 'Escape' && e.shiftKey && !net) {
      x.wf_app_finalize();
      replays.pump().then(() => (location.href = location.pathname));
      return;
    }
    if (e.code === 'Escape') {
      if (replays.open()) replays.close();
      else toggleSettings();
      return;
    }
    // F1: Classic 240x160 <-> PC wide (kept as the launcher's View setting too)
    if (e.code === 'F1') {
      e.preventDefault();
      const classic = (prefs.mode || 'gba:0').startsWith('gba');
      prefs.mode = classic ? prefs.last_pc || 'pc:0' : 'gba:0';
      if (classic === false) prefs.last_pc = $('mode').value;
      savePrefs();
      applyMode();
      const ls = lsGet('wf.launcher') || {};
      ls.view = prefs.mode;
      lsSet('wf.launcher', ls);
      x.wf_app_toast && withStr(prefs.mode.startsWith('gba') ? 'Classic 240x160 (F1)' : 'PC wide (F1)', (p, n) => x.wf_app_toast(p, n));
      return;
    }
    if (e.code === 'F3') {
      e.preventDefault();
      statsOn = !statsOn;
      $('stats').style.display = statsOn ? 'block' : 'none';
      return;
    }
    if (!running && (e.code === 'Enter' || e.code === 'Space')) {
      start();
      return;
    }
    if (replays.open()) {
      if (e.code === 'F10') replays.close();
      return;
    }
    if (settingsOpen()) return;
    const mods = (e.shiftKey ? 1 : 0) | (e.ctrlKey || e.metaKey ? 2 : 0) | (e.altKey ? 4 : 0);
    if (withStr(e.code, (p, n) => x.wf_app_key2(p, n, 1, 0, mods, e.repeat ? 1 : 0))) e.preventDefault();
  });
  addEventListener('keyup', (e) => withStr(e.code, (p, n) => x.wf_app_key2(p, n, 0, 0, 0, 0)));
  // the live recording is saved every 30 s and when the page goes away
  addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      x.wf_app_finalize();
      replays.pump();
    }
  });
  addEventListener('pagehide', () => {
    x.wf_app_finalize();
    replays.pump();
  });
  addEventListener('blur', () => x.wf_app_blur());

  // settings panel
  const settingsOpen = () => $('settings').classList.contains('on');
  function toggleSettings() {
    const open = !settingsOpen();
    $('settings').classList.toggle('on', open);
    x.wf_app_set_paused(open ? 1 : 0);
    x.wf_app_blur();
    if (open) {
      renderBinds();
      renderHotkeys();
      renderMods();
    }
  }
  function saveHotkeys() {
    x.wf_app_hotkeys();
    try {
      localStorage.setItem('wf.hotkeys', JSON.stringify(JSON.parse(str()).config));
    } catch {}
  }
  const keyLabel = (c) => c.replace(/(^|\+)(Key|Digit)/g, '$1').replace('Backquote', '`').replace('BracketLeft', '[').replace('BracketRight', ']').replace('Minus', '-').replace('Equal', '=');
  function renderHotkeys() {
    x.wf_app_hotkeys();
    const h = JSON.parse(str());
    $('hotkeys').replaceChildren(
      ...h.actions.map(([name, label, combos]) => {
        const row = document.createElement('div');
        row.className = 'row';
        const l = document.createElement('span');
        l.innerHTML = `<b>${label}</b> <span class="keys">${combos.map(keyLabel).join(', ') || '—'}</span>`;
        const btn = document.createElement('button');
        btn.textContent = rebindingHot === name ? 'Press keys…' : 'Rebind';
        if (rebindingHot === name) btn.className = 'wait';
        btn.onclick = () => {
          rebindingHot = name;
          renderHotkeys();
        };
        row.append(l, btn);
        return row;
      }),
    );
  }
  $('resethot').onclick = () => {
    withStr('', (ap, al) => withStr('', (cp, cl) => x.wf_app_rebind_hotkey(ap, al, cp, cl)));
    saveHotkeys();
    renderHotkeys();
  };
  $('hudbtn').onclick = toggleSettings;
  $('close').onclick = toggleSettings;
  $('mode').onchange = (e) => {
    prefs.mode = e.target.value;
    savePrefs();
    applyMode();
  };
  $('vol').value = prefs.vol ?? 1;
  $('resample').value = prefs.resample || 'linear';
  const pushAudio = () => audio?.node.port.postMessage({ type: 'set', gain: MUTED ? 0 : Number(prefs.vol ?? 1), linear: prefs.resample !== 'hold' });
  $('vol').oninput = (e) => {
    prefs.vol = Number(e.target.value);
    savePrefs();
    pushAudio();
  };
  $('resample').onchange = (e) => {
    prefs.resample = e.target.value;
    savePrefs();
    pushAudio();
  };
  function saveBindings() {
    x.wf_app_bindings();
    try {
      localStorage.setItem('wf.bindings', str());
    } catch {}
  }
  function renderBinds() {
    x.wf_app_bindings();
    const b = JSON.parse(str());
    const names = ['Up', 'Down', 'Left', 'Right', 'A', 'B', 'L', 'R', 'Start', 'Select'];
    $('binds').replaceChildren(
      ...names.map((n) => {
        const row = document.createElement('div');
        row.className = 'row';
        const arrows = { ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→' };
        const keys = b.keys.filter(([, v]) => v === n).map(([k]) => arrows[k] || k.replace(/^(Key|Digit)/, ''));
        const label = document.createElement('span');
        label.innerHTML = `<b>${n}</b> <span class="keys">${keys.join(', ') || '—'}</span>`;
        const btn = document.createElement('button');
        btn.textContent = rebinding === n ? 'Press a key…' : 'Rebind';
        if (rebinding === n) btn.className = 'wait';
        btn.onclick = () => {
          rebinding = n;
          renderBinds();
        };
        row.append(label, btn);
        return row;
      }),
    );
  }
  $('resetbinds').onclick = () => {
    x.wf_app_reset_bindings();
    saveBindings();
    renderBinds();
  };
  async function renderMods() {
    const m = await readMeta();
    const list = m.mods || [];
    $('mods').replaceChildren(
      ...(list.length
        ? list.map((name) => {
            const row = document.createElement('div');
            row.className = 'row';
            const btn = document.createElement('button');
            btn.textContent = 'Remove';
            btn.onclick = async () => {
              const mm = await readMeta();
              mm.mods = (mm.mods || []).filter((x2) => x2 !== name);
              await writeMeta(mm);
              await store.del(`mods/${name}.tar`);
              location.reload();
            };
            const s = document.createElement('span');
            s.textContent = name;
            row.append(s, btn);
            return row;
          })
        : [Object.assign(document.createElement('p'), { className: 'muted', textContent: 'No mods installed.' })]),
    );
    $('storeinfo').textContent = `Stored locally (${store.kind === 'opfs' ? 'Origin Private File System' : 'IndexedDB'}).`;
  }
  $('addmod').onclick = () => $('modpick').click();
  $('modpick').onchange = (e) => installMod([...e.target.files]);
  $('forget').onclick = async () => {
    if (!confirm('Delete the stored ROM, extracted content and mods from this browser?')) return;
    for (const k of await store.keys('')) await store.del(k);
    location.reload();
  };
  window.wf = { x, packs, timings, toggleSettings, replays, status: () => (x.wf_app_status(), JSON.parse(str())) };

  // ---------------------------------------------------------------- replays, save states (engine/docs/replays.md)
  // Storage: replays/<id>.wfr automatic recordings (newest N kept), kept/<id>.wfr kept + imported, states/slot<N>.wfs.
  function replaysUi() {
    let pumping = false;
    let wasPaused = false;
    const keepN = () => {
      x.wf_app_hotkeys();
      return JSON.parse(str()).config.keep_replays || 50;
    };
    const toast = (m) => withStr(m, (p, n) => x.wf_app_toast(p, n));
    const download = (bytes, name) => {
      const a = document.createElement('a');
      a.href = URL.createObjectURL(new Blob([bytes], { type: 'application/octet-stream' }));
      a.download = name;
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    };
    async function saveReplay(id, bytes) {
      const kept = await store.get(`kept/${id}.wfr`);
      await store.put(`${kept ? 'kept' : 'replays'}/${id}.wfr`, bytes);
      const auto = (await store.keys('replays/')).sort();
      const n = keepN();
      for (const k of auto.slice(0, Math.max(0, auto.length - n))) await store.del(k);
    }
    // drain the game's requests; buffers are read before any await (later calls overwrite them)
    let pumped = Promise.resolve();
    function pump() {
      if (!pumping) pumped = drain();
      return pumped;
    }
    async function drain() {
      pumping = true;
      try {
        for (let k = x.wf_app_next_request(); k; k = x.wf_app_next_request()) {
          const meta = JSON.parse(str() || '{}');
          const bytes = k === 1 || k === 2 ? blob() : null;
          if (k === 1) await saveReplay(meta.id, bytes);
          else if (k === 2) await store.put(`states/slot${meta.slot}.wfs`, bytes);
          else if (k === 3) {
            const b = await store.get(`states/slot${meta.slot}.wfs`);
            if (!b) toast(`State slot ${meta.slot} is empty`);
            else if (withBytes(b, (p, n) => x.wf_app_load_state(p, n))) toast(str());
          } else if (k === 4) show();
          // the battle's end screen: "Change fighter" -> the launcher's Battle page (after the recording is stored)
          else if (k === 5) {
            sessionStorage.setItem('wf.launcher.screen', 'battle');
            location.href = location.pathname;
          }
        }
      } catch (e) {
        console.error('replays', e);
      } finally {
        pumping = false;
      }
    }
    async function list() {
      const out = [];
      for (const [dir, kept] of [['kept/', true], ['replays/', false]]) {
        for (const key of await store.keys(dir)) {
          const b = await store.get(key);
          if (!b || withBytes(b, (p, n) => x.wf_app_replay_info(p, n))) continue;
          const id = key.slice(dir.length).replace(/\.wfr$/, '');
          if (!out.some((r) => r.id === id)) out.push({ id, key, kept, size: b.length, header: JSON.parse(str()) });
        }
      }
      return out.sort((a, b) => (a.id < b.id ? 1 : -1));
    }
    const fmt = (h) => {
      const s = h.frames / 59.7275;
      const d = h.created ? new Date(h.created * 1000).toLocaleString() : '';
      return `${d} · ${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}${h.outcome ? ' · ' + h.outcome : ''}${h.branch ? ' · branch of ' + h.branch.parent : ''}`;
    };
    async function render() {
      const items = await list();
      $('replaylist').replaceChildren(
        ...(items.length
          ? items.map((r) => {
              const row = document.createElement('div');
              row.className = 'row';
              const l = document.createElement('span');
              l.innerHTML = `<b>${r.header.title || r.id}</b>${r.kept ? ' ★' : ''}<br><span class="muted">${fmt(r.header)} · ${(r.size / 1024).toFixed(1)} KB</span>`;
              const btns = document.createElement('span');
              btns.className = 'btns';
              const b = (t, f, cls) => {
                const e = document.createElement('button');
                e.textContent = t;
                if (cls) e.className = cls;
                e.onclick = f;
                btns.append(e);
              };
              const bytes = () => store.get(r.key);
              b('Play', async () => {
                const by = await bytes();
                if (withStr(r.id, (ip, il) => withBytes(by, (p, n) => x.wf_app_play_replay(ip, il, p, n, 0)))) $('replayinfo').textContent = str();
                else close();
              });
              b('Ghost', async () => {
                const by = await bytes();
                if (withStr(r.id, (ip, il) => withBytes(by, (p, n) => x.wf_app_play_replay(ip, il, p, n, 1)))) $('replayinfo').textContent = str();
                else close();
              });
              b('Verify', async () => {
                const by = await bytes();
                const bad = withBytes(by, (p, n) => x.wf_app_verify(p, n));
                $('replayinfo').textContent = `${r.id}: ${bad ? str() : JSON.parse(str()).text}`;
              });
              b(r.kept ? 'Unkeep' : 'Keep', async () => {
                const by = await bytes();
                await store.put(`${r.kept ? 'replays' : 'kept'}/${r.id}.wfr`, by);
                await store.del(r.key);
                render();
              });
              b('Download', async () => download(await bytes(), `${r.id}.wfr`));
              b('Delete', async () => {
                await store.del(r.key);
                render();
              }, 'danger');
              row.append(l, btns);
              return row;
            })
          : [Object.assign(document.createElement('p'), { className: 'muted', textContent: 'No recordings yet: every fight is recorded automatically.' })]),
      );
      return items;
    }
    function show() {
      if (open()) return;
      x.wf_app_status();
      wasPaused = JSON.parse(str()).paused;
      x.wf_app_set_paused(1);
      x.wf_app_blur();
      $('replays').classList.add('on');
      $('replayinfo').textContent = '';
      // the current recording was queued by the request: save it first so it is listed
      setTimeout(() => pump().then(render), 0);
    }
    function close() {
      $('replays').classList.remove('on');
      x.wf_app_set_paused(wasPaused ? 1 : 0);
    }
    const open = () => $('replays').classList.contains('on');
    async function upload(files) {
      for (const f of files) {
        const by = new Uint8Array(await f.arrayBuffer());
        if (withBytes(by, (p, n) => x.wf_app_replay_info(p, n))) {
          $('replayinfo').textContent = `${f.name}: ${str()}`;
          continue;
        }
        const norm = blob();
        await store.put(`kept/${f.name.replace(/\.(wfr|json)$/i, '')}-import.wfr`, norm);
      }
      render();
    }
    $('replaysclose').onclick = close;
    $('replaysbtn').onclick = () => (open() ? close() : show());
    $('replayup').onclick = () => $('replaypick').click();
    $('replaypick').onchange = (e) => upload([...e.target.files]);
    $('replaycur').onclick = () => {
      if (x.wf_app_current_replay()) {
        $('replayinfo').textContent = 'Nothing is being recorded (a replay is playing).';
        return;
      }
      const meta = JSON.parse(str());
      download(blob(), `${meta.id}.wfr`);
    };
    // playback scrubber
    const scrub = $('scrub');
    setInterval(() => {
      if (!running) return;
      x.wf_app_status();
      const st = JSON.parse(str());
      const on = st.mode === 'playback';
      scrub.style.display = on ? 'block' : 'none';
      $('replaysbtn').style.display = st.enabled ? 'block' : 'none';
      if (on && document.activeElement !== scrub) {
        scrub.min = st.start;
        scrub.max = st.end;
        scrub.value = st.frame;
      }
    }, 250);
    scrub.oninput = () => x.wf_app_seek(Number(scrub.value));
    return { pump, show, close, open, list, render, upload, download };
  }
}
