// Runtime quality of the browser build (headless, muted): frame pacing, JS heap / GC pressure and input latency.
//   cd engine/web/tests && node perf_check.mjs [--seconds 20]
// Scenes: a story field (?story=8-1, walking) and a launcher battle (walking + punches), each at the default 960x640
// viewport and at 3840x2160 (the zoomed-out PC view at 4K). Per scene:
//   * the rAF interval distribution (p50 / p99 / max) and the frames longer than 1.5 display frames ("hitches")
//   * the page's own per-call timings (window.wfStats: step / render ms, sampled every 500 ms)
//   * main-thread long tasks (> 50 ms, PerformanceObserver) and the JS heap (performance.memory) start -> end
//   * input latency: a key pressed at a random moment (dispatched in the page, as a real keydown) -> the first rAF
//     in which the simulation has stepped past the frame it was on (the step that read the key): ms and display frames
// ROM: roms/buu.gba (or $WF_ROM); the wasm must be built (../app/build.sh).
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { launch, serve, waitFor, sleep, hold, tap } from './lib.mjs';
import { REPO } from './serve.mjs';

const args = process.argv.slice(2);
const SECS = args.includes('--seconds') ? Number(args[args.indexOf('--seconds') + 1]) : 20;
const ROM = process.env.WF_ROM || join(REPO, 'roms/buu.gba');
if (!existsSync(ROM)) {
  console.log(`no ROM at ${ROM}`);
  process.exit(2);
}
const { server, url } = await serve({ rom: ROM });
const { ctx } = await launch();

// the in-page sampler: rAF intervals, long tasks, heap, input latency probes
const INSTALL = () => {
  const s = { raf: [], long: [], heap0: performance.memory?.usedJSHeapSize ?? 0, lat: [], stats: [] };
  let last = performance.now();
  const tick = (t) => {
    s.raf.push(t - last);
    last = t;
    s.req = requestAnimationFrame(tick);
  };
  s.req = requestAnimationFrame(tick);
  try {
    new PerformanceObserver((l) => l.getEntries().forEach((e) => s.long.push(e.duration))).observe({ type: 'longtask', buffered: false });
  } catch {}
  s.statsT = setInterval(() => window.wfStats && s.stats.push({ step: window.wfStats.step_ms, render: window.wfStats.render_ms }), 500);
  window.__perf = s;
};
const PROBE = async (code) => {
  // a key at a random moment between frames, then wait for the sim to step past the frame it was on
  await new Promise((r) => setTimeout(r, Math.random() * 40));
  const x = window.wf.x;
  const f0 = x.wf_app_frame_no();
  const t0 = performance.now();
  dispatchEvent(new KeyboardEvent('keydown', { code, key: code, bubbles: true }));
  let frames = 0;
  await new Promise((res) => {
    const chk = () => {
      frames++;
      if (x.wf_app_frame_no() > f0) res();
      else requestAnimationFrame(chk);
    };
    requestAnimationFrame(chk);
  });
  const ms = performance.now() - t0;
  dispatchEvent(new KeyboardEvent('keyup', { code, key: code, bubbles: true }));
  window.__perf.lat.push({ ms, frames });
};
const REPORT = () => {
  const s = window.__perf;
  cancelAnimationFrame(s.req);
  clearInterval(s.statsT);
  const q = (v, p) => {
    const a = [...v].sort((a, b) => a - b);
    return a.length ? a[Math.round((a.length - 1) * p)] : 0;
  };
  const raf = s.raf.slice(2);
  const med = q(raf, 0.5);
  const lat = s.lat.map((l) => l.ms);
  const lf = s.lat.map((l) => l.frames);
  return {
    frames: raf.length,
    raf_ms: { p50: +med.toFixed(2), p99: +q(raf, 0.99).toFixed(2), max: +q(raf, 1).toFixed(2) },
    hitches: raf.filter((d) => d > med * 1.5).length,
    long_tasks: s.long.length,
    long_max_ms: +Math.max(0, ...s.long).toFixed(1),
    step_ms_max: +Math.max(0, ...s.stats.map((x) => x.step || 0)).toFixed(3),
    render_ms_p50: +q(s.stats.map((x) => x.render || 0), 0.5).toFixed(2),
    render_ms_max: +Math.max(0, ...s.stats.map((x) => x.render || 0)).toFixed(2),
    heap_mb: [+(s.heap0 / 1e6).toFixed(1), +((performance.memory?.usedJSHeapSize ?? 0) / 1e6).toFixed(1)],
    input_ms: { n: lat.length, p50: +q(lat, 0.5).toFixed(1), max: +q(lat, 1).toFixed(1), frames_p50: q(lf, 0.5), frames_max: q(lf, 1) },
    fps: +(window.wfStats?.fps ?? 0).toFixed(1),
  };
};

const scene = async (name, q, size, play) => {
  const page = await ctx.newPage();
  await page.setViewportSize(size);
  page.on('pageerror', (e) => console.log('[pageerror]', e.message));
  await page.goto(`${url}/app/${q}`);
  if (q === '') {
    await waitFor(page, () => window.wfLauncher === true, null, { what: 'launcher' });
    for (const k of ['ArrowDown', 'Enter', 'Enter']) {
      await page.evaluate((k) => window.wfLauncherKey(k), k);
      await sleep(150);
    }
  }
  await waitFor(page, () => window.wf && window.wf.status().frame > 60, null, { what: `${name} running`, timeout: 120000 });
  await page.evaluate(INSTALL);
  const t0 = Date.now();
  let k = 0;
  while (Date.now() - t0 < SECS * 1000) {
    await play(page, k++);
    if (k % 3 === 0) await page.evaluate(PROBE, 'KeyZ');
  }
  const r = await page.evaluate(REPORT);
  console.log(`${name} ${size.width}x${size.height}: ${JSON.stringify(r)}`);
  await page.close();
  return r;
};

try {
  // the first page extracts the ROM into the profile (cached for the rest)
  const p0 = await ctx.newPage();
  await p0.goto(`${url}/app/?devrom=/rom.gba`);
  await waitFor(p0, () => window.wfLauncher === true, null, { what: 'launcher after extraction' });
  await p0.close();
  const walk = async (page, k) => hold(page, ['ArrowRight', 'ArrowDown', 'ArrowLeft', 'ArrowUp'][k % 4], 40);
  const fight = async (page, k) => {
    await hold(page, k % 2 ? 'ArrowRight' : 'ArrowLeft', 20);
    await tap(page, 'KeyX', 6);
  };
  for (const size of [{ width: 960, height: 640 }, { width: 3840, height: 2160 }]) {
    await scene('story 8-1', '?story=8-1&autostart', size, walk);
    await scene('battle', '', size, fight);
  }
} finally {
  await ctx.close();
  server.close();
}
