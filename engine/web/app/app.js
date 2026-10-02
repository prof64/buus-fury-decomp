// Browser shell: ROM drop -> verify -> extract in a worker -> cache (OPFS / IndexedDB) -> run the game wasm
// (render + input on the main thread, the sound driver in an AudioWorklet). See engine/docs/frontend.md.
import { openStore } from './store.js';
import { tar, untar, CATEGORIES } from './tar.js';

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

async function loadMods() {
  const m = await readMeta();
  const out = [];
  for (const name of m.mods || []) {
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
  let bindings = null;
  try {
    bindings = JSON.parse(localStorage.getItem('wf.bindings'));
  } catch {}
  const cfg = { players: Number(qs.get('players') || 1), bindings };
  // real battle by default (?arena=forest&enemy=t68&count=1&fighter=goku&seed=1); ?demo or ?area=8-3 = demo world
  if (qs.has('demo') || qs.has('area')) Object.assign(cfg, { demo: true, area: qs.get('area') || '8-3' });
  for (const k of ['arena', 'enemy', 'fighter']) if (qs.get(k)) cfg[k] = qs.get(k);
  for (const k of ['count', 'form', 'level', 'seed']) if (qs.get(k)) cfg[k] = Number(qs.get(k));
  const t1 = performance.now();
  if (withStr(JSON.stringify(cfg), (p, n) => x.wf_app_init(p, n)) !== 0) {
    show('rom');
    $('romerr').textContent = `Could not start: ${str()}`;
    return;
  }
  timings.init_ms = Math.round(performance.now() - t1);

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
    const [k, z] = (prefs.mode || 'pc:0').split(':');
    x.wf_app_set_mode(k === 'gba' ? 1 : 0, Number(z));
    $('mode').value = prefs.mode || 'pc:0';
  };
  applyMode();
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
  show(qs.has('autostart') ? '' : 'start');
  const canvas = $('screen');
  const g = canvas.getContext('2d', { alpha: false });
  let running = false;
  let frames = 0;
  let fpsT = performance.now();
  let fps = 0;
  let statsOn = qs.has('stats');
  $('stats').style.display = statsOn ? 'block' : 'none';

  const start = async () => {
    if (running) return;
    running = true;
    show('');
    $('hudbtn').style.display = 'block';
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
  if (qs.has('autostart')) start();

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

  function frame() {
    pollPads();
    const dpr = window.devicePixelRatio || 1;
    const W = Math.floor(innerWidth * dpr);
    const H = Math.floor(innerHeight * dpr);
    x.wf_app_frame(W, H, 0);
    const w = x.wf_fb_w();
    const h = x.wf_fb_h();
    const s = x.wf_fb_scale();
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    canvas.style.width = `${(w * s) / dpr}px`;
    canvas.style.height = `${(h * s) / dpr}px`;
    g.putImageData(new ImageData(new Uint8ClampedArray(mem.buffer, x.wf_fb_ptr(), w * h * 4), w, h), 0, 0);
    const n = x.wf_app_audio_len();
    if (n) {
      const words = new Int32Array(mem.buffer, x.wf_app_audio_ptr(), n).slice();
      for (let i = 0; i + 3 < words.length; i += 4) window.wfAudioLog.push([x.wf_app_frame_no ? x.wf_app_frame_no() : 0, ...words.subarray(i, i + 4)]);
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
  addEventListener('keydown', (e) => {
    if (rebinding) {
      e.preventDefault();
      const btn = rebinding;
      rebinding = null;
      if (e.code !== 'Escape') withStr(btn, (bp, bl) => withStr(e.code, (cp, cl) => x.wf_app_rebind(bp, bl, cp, cl)));
      saveBindings();
      renderBinds();
      return;
    }
    if (e.code === 'Escape') {
      toggleSettings();
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
    if (withStr(e.code, (p, n) => x.wf_app_key(p, n, 1, 0))) e.preventDefault();
  });
  addEventListener('keyup', (e) => withStr(e.code, (p, n) => x.wf_app_key(p, n, 0, 0)));
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
      renderMods();
    }
  }
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
  $('modpick').onchange = async (e) => {
    const files = [...e.target.files];
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
  };
  $('forget').onclick = async () => {
    if (!confirm('Delete the stored ROM, extracted content and mods from this browser?')) return;
    for (const k of await store.keys('')) await store.del(k);
    location.reload();
  };
  window.wf = { x, packs, timings, toggleSettings };
}
