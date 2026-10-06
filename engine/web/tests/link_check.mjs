// Link play in the browser build between TWO headless Chromium pages on this machine (engine/docs/netcode.md
// "Link play: in the browser"): two separate browser processes and profiles (their own save slots in their own
// localStorage), a local signalling server on 127.0.0.1 only (wf-signal), the game served from 127.0.0.1. Each page
// runs `/app/?link=host|join:ROOM&bot=dojo|trade&quick&linkinfo` - the title with link play driven by the
// scripted player of the native --link-bot (menus -> Dojo match / trade) - and the test reads the link state back
// through window.wfLinkInfo(). Checks:
//   (a) Dojo: both pages confirm the same match (frame, checksum, inputs' hash) with mirrored endings;
//   (b) Trading Post: both report TRADE COMPLETE, and the supplies + zenie of the two saves (the trade store's
//       slot mirrored in localStorage) are conserved, each side holding the other's offer.
// Never opens a window; Chromium runs with --mute-audio.
//
//   cd engine/web/tests && npm install && node link_check.mjs [--build] [--only dojo|trade] [--verbose]
// --build runs ../app/build.sh first. Needs the native helpers built (it builds them: wf-signal, link_save) and
// extracted content for link_save ($WF_CONTENT, default engine/target/tmp/wf-core-field-content-2). Env: PORT
// (signalling, default 3593), CARGO_TARGET_DIR (default engine/target). Exit 0 = both flows passed.
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { chromium } from 'playwright-core';
import { chromiumPath, serve, waitFor, sleep } from './lib.mjs';
import { REPO, WEB } from './serve.mjs';

const args = process.argv.slice(2);
const ENGINE = join(REPO, 'engine');
const ROM = process.env.WF_ROM || join(REPO, 'roms/buu.gba');
const TARGET = process.env.CARGO_TARGET_DIR || join(ENGINE, 'target');
const CONTENT = process.env.WF_CONTENT || join(ENGINE, 'target/tmp/wf-core-field-content-2');
const PORT = Number(process.env.PORT || 3593);
const out = mkdtempSync(join(tmpdir(), 'wf-link-web-'));
if (!existsSync(ROM)) {
  console.log(`no ROM at ${ROM}`);
  process.exit(2);
}
if (!existsSync(join(CONTENT, 'pack.json'))) {
  console.log(`no extracted content at ${CONTENT} (set WF_CONTENT)`);
  process.exit(2);
}
if (args.includes('--build')) execFileSync('sh', [join(WEB, 'app/build.sh')], { stdio: 'inherit', env: process.env });
execFileSync('cargo', ['build', '--release', '--target-dir', TARGET, '-p', 'wf-net', '--features', 'server', '--bin', 'wf-signal', '-p', 'wf-app', '--example', 'link_save'], { cwd: ENGINE, stdio: 'inherit' });
const SIGNAL = join(TARGET, 'release/wf-signal');
const LINK_SAVE = join(TARGET, 'release/examples/link_save');

const results = [];
const check = (name, ok, detail = '') => {
  results.push({ name, ok });
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? ' - ' + detail : ''}`);
};

/// a slot (hex) from link_save: PARTY SUPPLIES ZENIE
function slotHex(tag, party, supplies, zenie) {
  const f = join(out, `${tag}.sav`);
  execFileSync(LINK_SAVE, ['write', CONTENT, f, party, supplies, String(zenie)]);
  return readFileSync(f).toString('hex');
}
/// a slot's supplies / zenie (link_save read)
function slotInfo(tag, hex) {
  const f = join(out, `${tag}.read.sav`);
  writeFileSync(f, Buffer.from(hex, 'hex'));
  return JSON.parse(execFileSync(LINK_SAVE, ['read', f], { encoding: 'utf8' }));
}

const sig = spawn(SIGNAL, [`127.0.0.1:${PORT}`], { stdio: 'ignore' });
const { server, url } = await serve({ rom: ROM });
const browsers = [];
const issues = [];

/// a headless, muted browser process with its own profile; the page with the content extracted (ROM drop)
async function player(who) {
  const profile = mkdtempSync(join(tmpdir(), `wf-link-${who}-`));
  const ctx = await chromium.launchPersistentContext(profile, {
    headless: true,
    executablePath: chromiumPath(),
    // (WebRTC between two browser processes on one machine: plain host candidates, no mDNS names)
    args: ['--mute-audio', '--autoplay-policy=no-user-gesture-required', '--disable-background-timer-throttling', '--disable-renderer-backgrounding',
      '--disable-features=WebRtcHideLocalIpsWithMdns'],
    viewport: { width: 960, height: 640 },
  });
  browsers.push(ctx);
  const page = await ctx.newPage();
  page.on('pageerror', (e) => issues.push(`${who} page error: ${e.message}`));
  page.on('console', (m) => m.type() === 'error' && issues.push(`${who} console: ${m.text().slice(0, 200)}`));
  await page.goto(`${url}/app/?devrom=/rom.gba`);
  await waitFor(page, () => window.wfLauncher === true || document.getElementById('romerr')?.textContent, null, { what: `${who}: content extracted` });
  return page;
}

/// run one flow: both pages on the link URL with their slot; until both report the flow's end
async function flow(kind, room, pages, slots, done) {
  const q = (role) => `?link=${role}:${room}&signal=ws://127.0.0.1:${PORT}&bot=${kind}&quick&linkinfo&name=${role}`;
  for (const [i, page] of pages.entries()) {
    await page.evaluate((h) => {
      for (const k of Object.keys(localStorage)) if (k.startsWith('wf.save.') || k.startsWith('wf.link.f.')) localStorage.removeItem(k);
      localStorage.setItem('wf.save.0', h);
    }, slots[i]);
  }
  await Promise.all(pages.map((p, i) => p.goto(`${url}/app/${q(i === 0 ? 'host' : 'join')}`)));
  const t0 = Date.now();
  let infos;
  for (;;) {
    infos = await Promise.all(pages.map((p) => p.evaluate(() => window.wfLinkInfo && window.wfLinkInfo()).catch(() => null)));
    if (infos.every((x) => x && done(x))) break;
    if (args.includes('--verbose') && (Date.now() - t0) % 10000 < 1000)
      console.log(`  ${kind} ${Math.round((Date.now() - t0) / 1000)} s:`, JSON.stringify(infos.map((x) => x && { phase: x.phase, last: x.last, trade: x.trade, cursor: x.cursor, setup: x.setup, title: x.title })));
    if (Date.now() - t0 > (Number(process.env.LINK_TIMEOUT_S) || 900) * 1000) {
      const j = await Promise.all(pages.map((p) => p.evaluate(() => localStorage.getItem('wf.link.f.saves/slot1.sav.trade.json')).catch(() => null)));
      throw new Error(`${kind}: no end: ${JSON.stringify(infos.map((x) => x && [x.phase, x.last]))} journals ${JSON.stringify(j)}`);
    }
    await sleep(1000);
  }
  console.log(`${kind}: ended after ${Math.round((Date.now() - t0) / 1000)} s`);
  // let the pages persist (slots every 500 ms)
  await sleep(1500);
  return infos;
}

try {
  const pages = [await player('host'), await player('guest')];
  const only = args.includes('--only') ? args[args.indexOf('--only') + 1] : null;

  if (only !== 'trade') {
  // ---- (a) the Dojo: In-Game mode is the bot's; the host (Goku 90) walks in punching, the guest (Vegeta 60) stands
  const dojo = await flow('dojo', 'WEBLINKD', pages, [slotHex('dh', '0:90', '-', 0), slotHex('dg', '4:60', '-', 0)], (x) => x.report && x.report.match);
  const [h, g] = dojo.map((x) => x.report.match);
  check('dojo: both pages confirm the same match (frame, checksum, inputs hash)', h.frame === g.frame && h.checksum === g.checksum && h.inputs_hash === g.inputs_hash,
    `frame ${h.frame} checksum ${h.checksum} / ${g.checksum} inputs ${h.inputs_hash} / ${g.inputs_hash}`);
  const head = (e) => e.split(/[ {(]/)[0];
  const pair = `${head(h.ending)}/${head(g.ending)}`;
  check('dojo: endings mirrored', ['Won/Lost', 'Lost/Won', 'Draw/Draw'].includes(pair), `${h.ending} | ${g.ending}`);
  check('dojo: a real match ran', h.frame > 60, `${h.frame} frames`);
  }
  if (only !== 'dojo') {
  // ---- (b) the Trading Post: host 5 of supply 0 + 500 zenie, guest 4 of supply 1 + 100 zenie
  const before = [slotHex('th', '0:20', '0:5', 500), slotHex('tg', '4:20', '1:4', 100)];
  const trade = await flow('trade', 'WEBLINKT', pages, before, (x) => x.last === 'complete' || x.phase === 'complete');
  check('trade: both report TRADE COMPLETE', trade.every((x) => x.last === 'complete' || x.phase === 'complete'), trade.map((x) => x.last || x.phase).join(' / '));
  const after = await Promise.all(pages.map((p) => p.evaluate(() => ({ disk: localStorage.getItem('wf.link.f.saves/slot1.sav'), slot: localStorage.getItem('wf.save.0'), journal: localStorage.getItem('wf.link.f.saves/slot1.sav.trade.json') }))));
  const b = before.map((x, i) => slotInfo(`b${i}`, x));
  const a = after.map((x, i) => slotInfo(`a${i}`, x.disk || ''));
  const total = (s) => {
    const t = { zenie: s.reduce((n, x) => n + x.zenie, 0) };
    for (const x of s) for (const [k, v] of Object.entries(x.supplies)) t[k] = (t[k] || 0) + v;
    return t;
  };
  check('trade: supplies + zenie conserved across the two saves', JSON.stringify(total(b)) === JSON.stringify(total(a)), `before ${JSON.stringify(total(b))} after ${JSON.stringify(total(a))}`);
  check('trade: something changed hands', JSON.stringify(b) !== JSON.stringify(a), `host ${JSON.stringify(a[0])} guest ${JSON.stringify(a[1])}`);
  // (the mirrored files are hex)
  const journals = after.map((x) => Buffer.from(x.journal || '', 'hex').toString());
  check('trade: the journal marks the trade committed on both', journals.every((j) => /"state":\s*"(Committed|Done)"/.test(j)), journals.map((j) => j.match(/"state":\s*"(\w+)"/)?.[1]).join(' / '));
  check('trade: the title slot (wf.save.0) holds the traded save on both', after.every((x) => x.slot && x.disk && x.slot === x.disk), after.map((x) => (x.slot === x.disk ? 'same' : 'differs')).join(' / '));
  }
} catch (e) {
  check('run', false, e.message);
} finally {
  for (const b of browsers) await b.close().catch(() => {});
  server.close();
  sig.kill();
}
if (issues.length) console.log(`page issues:\n  ${issues.slice(0, 10).join('\n  ')}`);
const bad = results.filter((r) => !r.ok).length;
console.log(bad ? `\n${bad} check(s) FAILED` : '\nall link checks passed');
process.exit(bad ? 1 : 0);
