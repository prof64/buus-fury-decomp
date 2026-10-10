// Link play between a NATIVE app and a BROWSER page in the same room (engine/docs/netcode.md "Link play: browser <->
// native"): one headless, muted `buus-fury --link host|join:ROOM --link-bot ...` process with its own --data folder
// and one headless, muted Chromium page `/app/?link=join|host:ROOM&bot=...`, on a local signalling server (127.0.0.1
// only). Both role assignments (native hosts / browser hosts). Checks:
//   (a) Dojo: both sides confirm the same match (frame, checksum, inputs' hash) with mirrored endings;
//   (b) Trading Post: both report TRADE COMPLETE; supplies + zenie conserved across the native slot file and the
//       browser's slot (the trade store's file mirrored in localStorage).
//
//   cd engine/web/tests && node link_mixed.mjs [--build] [--only dojo|trade] [--roles nh|bh]
// --build runs ../app/build.sh first (CARGO_TARGET_DIR honoured). Native binaries from CARGO_TARGET_DIR (default
// engine/target-linkmix): buus-fury, wf-signal, examples/link_save (built here). Env: PORT (default 3595), WF_CONTENT.
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { chromium } from 'playwright-core';
import { chromiumPath, serve, waitFor, sleep } from './lib.mjs';
import { REPO, WEB } from './serve.mjs';

const args = process.argv.slice(2);
const ENGINE = join(REPO, 'engine');
const ROM = process.env.WF_ROM || join(REPO, 'roms/buu.gba');
const TARGET = process.env.CARGO_TARGET_DIR || join(ENGINE, 'target-linkmix');
const CONTENT = process.env.WF_CONTENT || join(ENGINE, 'target/tmp/wf-core-field-content-2');
const PORT = Number(process.env.PORT || 3595);
const out = mkdtempSync(join(tmpdir(), 'wf-link-mixed-'));
if (!existsSync(ROM) || !existsSync(join(CONTENT, 'pack.json'))) {
  console.log(`need the ROM (${ROM}) and extracted content (${CONTENT})`);
  process.exit(2);
}
if (args.includes('--build')) execFileSync('sh', [join(WEB, 'app/build.sh')], { stdio: 'inherit', env: { ...process.env, CARGO_TARGET_DIR: TARGET } });
execFileSync('cargo', ['build', '--release', '--target-dir', TARGET, '-p', 'wf-native'], { cwd: ENGINE, stdio: 'inherit' });
execFileSync('cargo', ['build', '--release', '--target-dir', TARGET, '-p', 'wf-app', '--example', 'link_save', '--example', 'dojo_replay'], { cwd: ENGINE, stdio: 'inherit' });
execFileSync('cargo', ['build', '--release', '--target-dir', TARGET, '-p', 'wf-net', '--features', 'server', '--bin', 'wf-signal'], { cwd: ENGINE, stdio: 'inherit' });
const BIN = join(TARGET, 'release/buus-fury');
const SIGNAL = join(TARGET, 'release/wf-signal');
const LINK_SAVE = join(TARGET, 'release/examples/link_save');

const results = [];
const check = (name, ok, detail = '') => {
  results.push({ name, ok });
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? ' - ' + detail : ''}`);
};
const slotFile = (tag, party, supplies, zenie) => {
  const f = join(out, `${tag}.sav`);
  execFileSync(LINK_SAVE, ['write', CONTENT, f, party, supplies, String(zenie)]);
  return f;
};
const slotRead = (file) => JSON.parse(execFileSync(LINK_SAVE, ['read', file], { encoding: 'utf8' }));
const slotReadHex = (tag, hex) => {
  const f = join(out, `${tag}.read.sav`);
  writeFileSync(f, Buffer.from(hex, 'hex'));
  return slotRead(f);
};

const sig = spawn(SIGNAL, [`127.0.0.1:${PORT}`], { stdio: 'ignore' });
const { server, url } = await serve({ rom: ROM });
const issues = [];
const procs = [];
let ctx = null;

async function browserPage() {
  const profile = mkdtempSync(join(tmpdir(), 'wf-link-mixed-page-'));
  ctx = await chromium.launchPersistentContext(profile, {
    headless: true,
    executablePath: chromiumPath(),
    args: ['--mute-audio', '--autoplay-policy=no-user-gesture-required', '--disable-background-timer-throttling', '--disable-renderer-backgrounding',
      '--disable-features=WebRtcHideLocalIpsWithMdns'],
    viewport: { width: 960, height: 640 },
  });
  const page = await ctx.newPage();
  page.on('pageerror', (e) => issues.push(`page error: ${e.message}`));
  page.on('console', (m) => m.type() === 'error' && issues.push(`console: ${m.text().slice(0, 200)}`));
  await page.goto(`${url}/app/?devrom=/rom.gba`);
  await waitFor(page, () => window.wfLauncher === true || document.getElementById('romerr')?.textContent, null, { what: 'content extracted' });
  return page;
}

/// one flow: the native app as `nrole`, the page as the other role; until both report the flow's end.
/// -> { native: report json, page: wfLinkInfo(), dataDir, pageAfter: localStorage files }
async function flow(page, kind, room, nrole, nslot, pslotHex, done) {
  const brole = nrole === 'host' ? 'join' : 'host';
  const data = join(out, `${kind}-${nrole}-native`);
  mkdirSync(join(data, 'saves'), { recursive: true });
  writeFileSync(join(data, 'saves/slot1.sav'), readFileSync(nslot));
  await page.evaluate((h) => {
    for (const k of Object.keys(localStorage)) if (k.startsWith('wf.save.') || k.startsWith('wf.link.f.')) localStorage.removeItem(k);
    localStorage.setItem('wf.save.0', h);
  }, pslotHex);
  const log = join(out, `${kind}-${nrole}-native.log`);
  const p = spawn(BIN, ['--content', CONTENT, '--data', data, '--headless', '--mute', '--realtime', '--frames', '60000',
    '--link', `${nrole}:${room}`, '--link-bot', kind, '--link-quick', '--signal', `ws://127.0.0.1:${PORT}`, '--net-name', `native-${nrole}`],
  { stdio: ['ignore', 'pipe', 'pipe'] });
  procs.push(p);
  const logs = [];
  p.stdout.on('data', (d) => logs.push(d));
  p.stderr.on('data', (d) => logs.push(d));
  await page.goto(`${url}/app/?link=${brole}:${room}&signal=ws://127.0.0.1:${PORT}&bot=${kind}&quick&linkinfo&name=browser-${brole}`);
  const t0 = Date.now();
  const rep = join(data, 'link_report.json');
  let info;
  for (;;) {
    info = await page.evaluate(() => window.wfLinkInfo && window.wfLinkInfo()).catch(() => null);
    const nat = existsSync(rep) && readFileSync(rep, 'utf8').trim() ? JSON.parse(readFileSync(rep, 'utf8')) : null;
    if (info && done(info) && nat) {
      await sleep(1500);
      p.kill();
      writeFileSync(log, Buffer.concat(logs));
      const pageAfter = await page.evaluate(() => ({ disk: localStorage.getItem('wf.link.f.saves/slot1.sav') }));
      console.log(`${kind} (native ${nrole}): ended after ${Math.round((Date.now() - t0) / 1000)} s`);
      return { native: JSON.parse(readFileSync(rep, 'utf8')), page: info, data, pageAfter };
    }
    if (Date.now() - t0 > (Number(process.env.LINK_TIMEOUT_S) || 900) * 1000) {
      p.kill();
      writeFileSync(log, Buffer.concat(logs));
      throw new Error(`${kind} (native ${nrole}): no end - page ${JSON.stringify(info && [info.phase, info.last])} native report ${nat ? 'yes' : 'no'} (log ${log})`);
    }
    await sleep(1000);
  }
}

try {
  const page = await browserPage();
  const only = args.includes('--only') ? args[args.indexOf('--only') + 1] : null;
  const roles = args.includes('--roles') ? [args[args.indexOf('--roles') + 1]] : ['nh', 'bh'];
  for (const r of roles) {
    const nrole = r === 'nh' ? 'host' : 'join';
    const tag = `native ${nrole === 'host' ? 'hosts' : 'joins'}`;
    if (only !== 'trade') {
      // the Dojo: whoever hosts is Goku 90 (walks in punching), the joiner Vegeta 60 (stands) - as the other tests
      const hostSlot = slotFile(`dh${r}`, '0:90', '-', 0);
      const joinSlot = slotFile(`dg${r}`, '4:60', '-', 0);
      const nslot = nrole === 'host' ? hostSlot : joinSlot;
      const pslot = readFileSync(nrole === 'host' ? joinSlot : hostSlot).toString('hex');
      const res = await flow(page, 'dojo', `MIXD${r.toUpperCase()}`, nrole, nslot, pslot, (x) => x.report && x.report.match);
      const n = res.native.match;
      const b = res.page.report.match;
      check(`dojo (${tag}): same match on both (frame, checksum, inputs hash)`, n && b && n.frame === b.frame && n.checksum === b.checksum && n.inputs_hash === b.inputs_hash,
        `native ${n && [n.frame, n.checksum, n.inputs_hash]} / browser ${b && [b.frame, b.checksum, b.inputs_hash]}`);
      const head = (e) => e.split(/[ {(]/)[0];
      check(`dojo (${tag}): endings mirrored`, ['Won/Lost', 'Lost/Won', 'Draw/Draw'].includes(`${head(n.ending)}/${head(b.ending)}`), `${n.ending} | ${b.ending}`);
      check(`dojo (${tag}): a real match ran`, n.frame > 60, `${n.frame} frames`);
      writeFileSync(join(out, `dojo-${r}.json`), JSON.stringify({ native: res.native, browser: res.page.report }, null, 1));
      // (b) same inputs -> identical state, per frame: each side's confirmed per-frame checksums, then each side's
      // recording replayed on the OTHER build (native example dojo_replay / the page's wfDojoReplay)
      const nh = n.replay.hashes;
      const bh = b.replay.hashes;
      check(`dojo (${tag}): per-frame checksums native == browser (${nh.length} frames)`, nh.length > 60 && JSON.stringify(nh) === JSON.stringify(bh),
        `first difference ${nh.findIndex((x, i) => x !== bh[i])}`);
      const brep = join(out, `dojo-${r}-browser-report.json`);
      writeFileSync(brep, JSON.stringify(res.page));
      let nat;
      try {
        nat = execFileSync(join(TARGET, 'release/examples/dojo_replay'), [CONTENT, brep], { encoding: 'utf8' }).trim();
      } catch (e) {
        nat = `${e.stdout || ''}${e.stderr || ''}`.trim();
      }
      check(`dojo (${tag}): the browser's recording replayed natively - identical every frame`, nat.startsWith('identical'), nat);
      const w = await page.evaluate((rp) => window.wfDojoReplay && window.wfDojoReplay(rp), n.replay);
      check(`dojo (${tag}): the native recording replayed in the browser (wasm) - identical every frame`, w && !w.error && w.first === null && w.hashes.length === nh.length,
        w ? (w.error || `first difference ${w.first}, ${w.hashes.length} frames`) : 'no wfDojoReplay');
    }
    if (only !== 'dojo') {
      const hostSlot = slotFile(`th${r}`, '0:20', '0:5', 500);
      const joinSlot = slotFile(`tg${r}`, '4:20', '1:4', 100);
      const nslot = nrole === 'host' ? hostSlot : joinSlot;
      const pslotFile = nrole === 'host' ? joinSlot : hostSlot;
      const res = await flow(page, 'trade', `MIXT${r.toUpperCase()}`, nrole, nslot, readFileSync(pslotFile).toString('hex'),
        (x) => x.last === 'complete' || x.phase === 'complete');
      check(`trade (${tag}): TRADE COMPLETE on both`, res.native.last === 'complete' && (res.page.last === 'complete' || res.page.phase === 'complete'),
        `native ${res.native.last} / browser ${res.page.last || res.page.phase}`);
      const before = [slotRead(nslot), slotRead(pslotFile)];
      const after = [slotRead(join(res.data, 'saves/slot1.sav')), slotReadHex(`ta${r}`, res.pageAfter.disk || '')];
      const total = (s) => {
        const t = { zenie: s.reduce((n, x) => n + x.zenie, 0), equipment: s.reduce((n, x) => n + x.equipment, 0) };
        for (const x of s) for (const [k, v] of Object.entries(x.supplies)) t[k] = (t[k] || 0) + v;
        return JSON.stringify(Object.keys(t).sort().map((k) => [k, t[k]]));
      };
      check(`trade (${tag}): supplies + zenie conserved across the native file and the browser save`, total(before) === total(after), `before ${total(before)} after ${total(after)}`);
      check(`trade (${tag}): something changed hands`, JSON.stringify(before) !== JSON.stringify(after), `native ${JSON.stringify(after[0])} browser ${JSON.stringify(after[1])}`);
    }
  }
} catch (e) {
  check('run', false, e.message);
} finally {
  for (const p of procs) p.kill();
  if (ctx) await ctx.close().catch(() => {});
  server.close();
  sig.kill();
}
if (issues.length) console.log(`page issues:\n  ${issues.slice(0, 10).join('\n  ')}`);
console.log(`files: ${out}`);
const bad = results.filter((r) => !r.ok).length;
console.log(bad ? `\n${bad} check(s) FAILED` : '\nall mixed link checks passed');
process.exit(bad ? 1 : 0);
