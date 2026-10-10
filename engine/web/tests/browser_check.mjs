// Headless, muted end-to-end check of the browser build (engine/web/app + the two wasm binaries):
//   1. a clean browser profile: the ROM dropped (?devrom), extracted in the worker, stored (OPFS / IndexedDB)
//   2. launcher -> Story -> Title Screen -> New Game, a scripted few thousand frames of play
//   3. the session's recording (wf_app_current_replay) verified by the native build (buus-fury --replay: the
//      browser's per-interval state hashes against a native session fed the same inputs) and by the page itself
//   4. story save: ?story=8-1, walk to the save point, save; page reload (content from the cache, slots and options
//      from browser storage) -> launcher -> Story -> the slot: the game continues where it was saved
//   5. replays / rewind / save states in the page: rewind, save state, load state, replay playback + verify
//   6. a battle from the launcher, its recording verified natively
// Never opens a window; Chromium runs with --mute-audio and the page mutes itself under automation.
//
//   cd engine/web/tests && npm install && node browser_check.mjs [--build] [--keep DIR]
// --build runs ../app/build.sh first (CARGO_TARGET_DIR honoured); the native binary is built with cargo
// (release, CARGO_TARGET_DIR or engine/target). ROM: roms/buu.gba (or $WF_ROM). Exit 0 = every check passed.
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { launch, serve, waitFor, sleep, tap, hold, status, blobOf, save } from './lib.mjs';
import { REPO, WEB } from './serve.mjs';

const args = process.argv.slice(2);
const ENGINE = join(REPO, 'engine');
const ROM = process.env.WF_ROM || join(REPO, 'roms/buu.gba');
const TARGET = process.env.CARGO_TARGET_DIR || join(ENGINE, 'target');
const out = args.includes('--keep') ? args[args.indexOf('--keep') + 1] : mkdtempSync(join(tmpdir(), 'wf-web-check-'));
mkdirSync(out, { recursive: true });
// --only 4,6: run those parts (1, the ROM drop, always runs: the profile needs the content)
const only = args.includes('--only') ? args[args.indexOf('--only') + 1].split(',').map(Number) : null;
const want = (n) => !only || only.includes(n);
const results = [];
const check = (name, ok, detail = '') => {
  results.push({ name, ok, detail });
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? ' - ' + detail : ''}`);
};
const step = (s) => console.log(`\n== ${s}`);

if (!existsSync(ROM)) {
  console.log(`no ROM at ${ROM}`);
  process.exit(2);
}
if (args.includes('--build')) execFileSync('sh', [join(WEB, 'app/build.sh')], { stdio: 'inherit', env: process.env });
if (!existsSync(join(WEB, 'app/wf_web.wasm')) || !existsSync(join(WEB, 'wf_wasm.wasm'))) {
  console.log('the wasm binaries are missing: run with --build (or engine/web/app/build.sh)');
  process.exit(2);
}
// the native side: buus-fury with its own data dir (content extracted from the same ROM)
execFileSync('cargo', ['build', '--release', '-p', 'wf-native', '--target-dir', TARGET], { cwd: ENGINE, stdio: 'inherit' });
const NATIVE = join(TARGET, 'release/buus-fury');
const DATA = join(out, 'native-data');
const native = (...a) => spawnSync(NATIVE, ['--data', DATA, ROM, '--headless', ...a], { encoding: 'utf8' });
const nativeVerify = (file) => {
  const r = native('--replay', file);
  const line = (r.stderr || '').split('\n').find((l) => l.startsWith('verify ')) || (r.stderr || '').trim().split('\n').pop();
  return { ok: / OK \(/.test(line || ''), line };
};

const { server, url } = await serve({ rom: ROM });
const { ctx } = await launch();
const pages = [];
const issues = [];
const open = async (q) => {
  const page = await ctx.newPage();
  pages.push(page);
  page.on('pageerror', (e) => issues.push(`page error: ${e.message}`));
  page.on('console', (m) => (m.type() === 'error' || m.type() === 'warning') && issues.push(`console ${m.type()}: ${m.text().slice(0, 300)}`));
  await page.goto(`${url}/app/${q}`);
  return page;
};
const story = async (page) => {
  const s = await page.evaluate(() => {
    window.wf.x.wf_app_story_info?.();
    const x = window.wf.x;
    return new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.wf_str_ptr(), x.wf_str_len()));
  });
  return JSON.parse(s);
};
const stats = (page) => page.evaluate(() => window.wfStats);
const launcher = async (page, keys) => {
  await waitFor(page, () => window.wfLauncher === true, null, { what: 'launcher' });
  for (const k of keys) {
    await page.evaluate((k) => window.wfLauncherKey(k), k);
    await sleep(150);
  }
  await waitFor(page, () => window.wf && window.wf.status().frame > 0, null, { what: 'game running', timeout: 60000 });
};
const pageVerify = async (page, bytes) =>
  page.evaluate((b64) => {
    const x = window.wf.x;
    const b = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const p = x.wf_alloc(b.length);
    new Uint8Array(x.memory.buffer, p, b.length).set(b);
    const bad = x.wf_app_verify(p, b.length);
    x.wf_dealloc(p, b.length);
    const s = new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.wf_str_ptr(), x.wf_str_len()));
    return bad ? { ok: false, text: s } : JSON.parse(s);
  }, bytes.toString('base64'));
const action = (page, name) => page.evaluate((n) => {
  const x = window.wf.x;
  const b = new TextEncoder().encode(n);
  const p = x.wf_alloc(b.length);
  new Uint8Array(x.memory.buffer, p, b.length).set(b);
  const r = x.wf_app_action(p, b.length);
  x.wf_dealloc(p, b.length);
  return r;
}, name);
const frames = (n) => sleep((n * 1000) / 59.7275);

try {
  // ------------------------------------------------------------------------------------------------ 1. ROM drop
  step('1. ROM drop on a clean profile');
  let t0 = Date.now();
  let page = await open('?devrom=/rom.gba');
  await waitFor(page, () => window.wfLauncher === true || document.getElementById('romerr')?.textContent, null, { what: 'launcher after extraction' });
  const err = await page.evaluate(() => document.getElementById('romerr')?.textContent);
  check('ROM extracted, launcher up', !err, err || `${Date.now() - t0} ms ${JSON.stringify(await page.evaluate(() => window.wfTimings))}`);
  const kinds = await page.evaluate(async () => {
    const root = await navigator.storage.getDirectory().catch(() => null);
    const names = [];
    if (root) for await (const [k] of root.entries()) names.push(k);
    return names;
  });
  check('content cached in browser storage', kinds.length > 0, kinds.join(', '));

  // ------------------------------------------------------------------------------ 2. Story -> title -> New Game
  if (want(2) || want(3) || want(5)) {
  step('2. launcher -> Story -> Title Screen -> New Game, a few thousand frames');
  await launcher(page, ['Enter', 'Enter']); // Story, Title Screen
  await frames(780); // boot screens + title
  for (const k of ['Enter', 'Enter', 'KeyX', 'KeyX']) {
    await tap(page, k, 50);
  } // Start, Single Player, Game 1 (New Game), confirm
  // the prologue's text: A through it; then walk around (whatever the scene allows)
  const script = [];
  for (let i = 0; i < 40; i++) script.push(['KeyX', 3, 40]);
  for (const k of ['ArrowRight', 'ArrowDown', 'ArrowLeft', 'ArrowUp']) script.push([k, 90, 10], ['KeyX', 3, 20]);
  for (const [k, n, after] of script) {
    await hold(page, k, n);
    await frames(after);
  }
  const s2 = await status(page);
  const st2 = await stats(page);
  const info2 = await story(page);
  check('story running after New Game', s2.frame > 2500 && info2 && info2.area, `frame ${s2.frame}, area ${JSON.stringify(info2?.area)} p ${JSON.stringify(info2?.p)} hash ${st2?.state_hash}`);

  // ------------------------------------------------------------- 3. the recording vs the native build
  step('3. the browser session vs a native session on the same inputs (state hashes)');
  const rep = await blobOf(page, 'wf_app_current_replay');
  check('recording exported', !!rep, rep ? `${rep.length} bytes` : 'none');
  if (rep) {
    const f = save(out, 'story.wfr', rep);
    const nv = nativeVerify(f);
    check('native replay of the browser session matches (every checkpoint hash)', nv.ok, nv.line);
    const pv = await pageVerify(page, rep);
    check('page re-verifies its own recording', !!pv.ok, pv.text);
  }

  // ------------------------------------------------------------------ 5. rewind / save states / replay playback
  step('5. rewind, save states, replay playback in the page');
  const f0 = (await status(page)).frame;
  await page.keyboard.down('Backquote');
  await frames(120);
  await page.keyboard.up('Backquote');
  await frames(5);
  const f1 = (await status(page)).frame;
  check('rewind goes back', f1 < f0, `${f0} -> ${f1}`);
  await action(page, 'save_state_1');
  await page.evaluate(() => window.wf.replays.pump());
  const hashSaved = (await page.evaluate(() => (window.wf.x.wf_app_stats(), window.wf.status()))).frame;
  const hs = (await stats(page))?.state_hash;
  await hold(page, 'ArrowRight', 60);
  await frames(40);
  await action(page, 'load_state_1');
  await page.evaluate(() => window.wf.replays.pump());
  await frames(10);
  const sAfter = await status(page);
  check('save state + load state', sAfter.frame >= hashSaved && sAfter.frame < hashSaved + 60, `saved at ${hashSaved}, after load ${sAfter.frame} (hash then ${hs})`);
  const rep2 = await blobOf(page, 'wf_app_current_replay');
  if (rep2) {
    const f = save(out, 'story_branch.wfr', rep2);
    const nv = nativeVerify(f);
    check('native replay of the recording after rewind / state load', nv.ok, nv.line);
    const ok = await page.evaluate((b64) => {
      const x = window.wf.x;
      const b = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const id = new TextEncoder().encode('check');
      const p = x.wf_alloc(b.length), q = x.wf_alloc(id.length);
      new Uint8Array(x.memory.buffer, p, b.length).set(b);
      new Uint8Array(x.memory.buffer, q, id.length).set(id);
      return x.wf_app_play_replay(q, id.length, p, b.length, 0) === 0;
    }, rep2.toString('base64'));
    await frames(60);
    const sp = await status(page);
    check('replay playback in the page', ok && sp.mode === 'playback', JSON.stringify({ mode: sp.mode, frame: sp.frame, desync: sp.desync }));
  }
  }
  await page.close();

  // ------------------------------------------------------------------------- 4. save, reload, Continue
  if (want(4)) {
  step('4. in-game save at 8-1, page reload, Continue');
  page = await open('?story=8-1&autostart');
  await waitFor(page, () => window.wf && window.wf.status().frame > 30, null, { what: 'story 8-1' });
  await frames(60);
  const i0 = await story(page);
  console.log('8-1 start', JSON.stringify(i0));
  // the save point: rect (128, 152, 57 x 41) on 8-1, A facing it
  const target = { x: 150, y: 175 };
  for (let tries = 0; tries < 12; tries++) {
    const i = await story(page);
    const [x, y] = i.p;
    const dx = target.x - x, dy = target.y - y;
    if (Math.abs(dx) < 6 && Math.abs(dy) < 6) break;
    if (Math.abs(dx) >= 6) await hold(page, dx > 0 ? 'ArrowRight' : 'ArrowLeft', Math.min(60, Math.abs(dx) / 1.5));
    if (Math.abs(dy) >= 6) await hold(page, dy > 0 ? 'ArrowDown' : 'ArrowUp', Math.min(60, Math.abs(dy) / 1.5));
    await frames(5);
  }
  const iNear = await story(page);
  console.log('at the save point', JSON.stringify(iNear));
  const gen0 = await page.evaluate(() => window.wf.x.wf_app_saves());
  await tap(page, 'KeyX', 60); // the save point's event
  for (let i = 0; i < 8; i++) await tap(page, 'KeyX', 40); // through the menu / the Save Game screen / "Game Saved"
  await frames(120);
  const gen1 = await page.evaluate(() => window.wf.x.wf_app_saves());
  const saved = await page.evaluate(() => [0, 1, 2, 3].map((i) => (localStorage.getItem(`wf.save.${i}`) || '').length));
  const iSaved = await story(page);
  check('in-game save reached browser storage', gen1 !== gen0 && saved[0] > 0, `generation ${gen0} -> ${gen1}, stored lengths ${saved}, ${JSON.stringify(iSaved)}`);
  check('options block stored with the slots', saved[3] > 0, `wf.save.3 length ${saved[3]}`);
  // options other than the defaults in the stored block (text speed 6, SFX 4, music 8 - the game's flash record:
  // 4 bytes, adler32 + 0x12345678 LE): the reloaded game must read them
  await page.evaluate(() => {
    const d = [6, 4, 8, 0];
    let a = 1, b = 0;
    for (const v of d) { a = (a + v) % 65521; b = (b + a) % 65521; }
    const c = ((b << 16) | a) + 0x12345678 >>> 0;
    const bytes = [...d, c & 255, (c >>> 8) & 255, (c >>> 16) & 255, c >>> 24];
    localStorage.setItem('wf.save.3', bytes.map((x) => x.toString(16).padStart(2, '0')).join(''));
  });
  await page.close();
  page = await open('');
  await launcher(page, ['Enter', 'ArrowDown', 'Enter']); // Story, slot 1
  await frames(120);
  const iCont = await story(page);
  check('options from browser storage after a reload', JSON.stringify(iCont?.options) === '[6,4,8]', JSON.stringify(iCont?.options));
  check('Continue from the slot after a reload', !!iCont && JSON.stringify(iCont.area) === JSON.stringify(iSaved.area) && Math.abs(iCont.p[0] - iSaved.p[0]) < 4 && Math.abs(iCont.p[1] - iSaved.p[1]) < 4, `${JSON.stringify(iCont)} vs saved ${JSON.stringify(iSaved)}`);
  await page.close();

  }

  // -------------------------------------------------------------------------------------------- 6. a battle
  if (want(6)) {
  step('6. a battle from the launcher');
  page = await open('');
  await launcher(page, ['ArrowDown', 'Enter', 'Enter']); // Battle (the cursor opens on START), START
  for (let i = 0; i < 60; i++) {
    await hold(page, i % 2 ? 'ArrowRight' : 'ArrowLeft', 30);
    await tap(page, 'KeyX', 10);
  }
  const sb = await status(page);
  check('battle running', sb.frame > 2000, `frame ${sb.frame}`);
  const rb = await blobOf(page, 'wf_app_current_replay');
  if (rb) {
    const nv = nativeVerify(save(out, 'battle.wfr', rb));
    check('native replay of the battle matches', nv.ok, nv.line);
  } else check('battle recording exported', false);
  await page.close();
  }
  check('no page errors or console warnings', issues.length === 0, issues.slice(0, 5).join(' | '));
} catch (e) {
  check('harness', false, e.stack || String(e));
} finally {
  await ctx.close();
  server.close();
}
const bad = results.filter((r) => !r.ok);
console.log(`\n${results.length - bad.length} / ${results.length} checks passed (files in ${out})`);
process.exit(bad.length ? 1 : 0);
