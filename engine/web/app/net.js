// Online multiplayer in the browser (engine/docs/netcode.md). The lobby menu, the lobby and the rollback session run
// in the game wasm (wf_menu_* / wf_net_* exports, crates/wf-web/src/net.rs); this module is the transport: the
// Matchbox signaling protocol over a WebSocket and one RTCDataChannel per peer configured exactly like
// matchbox_socket's unreliable channel (negotiated id 0, unordered, no retransmits), so browser and native
// (buus-fury) peers meet in one room. ICE: STUN / TURN from ?ice= or the signaling server's /ice (time-limited TURN
// credentials), ?relay forces the TURN relay. A match that loses every other player reconnects and rejoins in place.
//
// URL: ?net (the in-game lobby menu) or ?net=host|join|spectate|rejoin|synctest&room=CODE[-TOKEN]&signal=wss://host
// &key=K&players=2&pvp&fighter=vegeta&form=26&enemy=t60&arena=wasteland&count=1&seed=1&delay=2&pause=shared
// &sfx=reconcile&ice=[...]|auto|none&relay&latency=0&loss=0&name=...

const dec = new TextDecoder();
const enc = new TextEncoder();

const ENEMY_ARENA = {
  t68: 'forest', t60: 'wasteland', t51: 'crater', 't26+~t29': 'crater', t129: 'crater', t145: 'yakon', t10: 'volcano', t18: 'demon',
};

const css = `
#netstat{position:fixed;left:50%;transform:translateX(-50%);top:10px;z-index:6;font:12px/1.4 ui-monospace,monospace;background:rgba(0,0,0,.6);padding:5px 8px;border-radius:6px;white-space:pre;pointer-events:auto}
#netstat .bad{color:var(--bad);font-weight:700}#netstat a{color:var(--accent)}`;

function el(html) {
  const t = document.createElement('template');
  t.innerHTML = html.trim();
  return t.content.firstChild;
}

const store = {
  get(k) {
    try {
      return localStorage.getItem(k);
    } catch {
      return null;
    }
  },
  set(k, v) {
    try {
      if (v == null) localStorage.removeItem(k);
      else localStorage.setItem(k, v);
    } catch {}
  },
};

function secret() {
  const b = new Uint8Array(24);
  crypto.getRandomValues(b);
  return [...b].map((x) => x.toString(16).padStart(2, '0')).join('');
}

const isLocal = (signal) => /^wss?:\/\/(127\.0\.0\.1|localhost|\[::1\])(:|\/|$)/.test(signal);
const httpOrigin = (signal) => signal.replace(/^ws(s?):\/\//, 'http$1://').replace(/^(https?:\/\/[^/]+).*$/, '$1');

/// The ICE servers: ?ice=JSON|url,url|auto|none, else the signaling server's /ice (auto) for non-local servers.
async function iceConfig(cfg) {
  const spec = cfg.ice ?? (isLocal(cfg.signal) ? 'none' : 'auto');
  let servers = [];
  if (spec === 'none' || spec === '') servers = [];
  else if (spec === 'auto') {
    try {
      const r = await fetch(`${httpOrigin(cfg.signal)}/ice${cfg.key ? `?key=${encodeURIComponent(cfg.key)}` : ''}`, { cache: 'no-store' });
      if (r.ok) servers = (await r.json()).iceServers || [];
    } catch (e) {
      console.warn('wf-net: no /ice from the signaling server', e);
    }
    if (!servers.length) servers = [{ urls: 'stun:stun.l.google.com:19302' }];
  } else if (spec.trim().startsWith('[') || spec.trim().startsWith('{')) {
    const v = JSON.parse(spec);
    servers = Array.isArray(v) ? v : v.iceServers || [v];
  } else {
    const urls = spec.split(',').map((s) => s.trim()).filter(Boolean);
    const turn = urls.filter((u) => u.startsWith('turn'));
    const stun = urls.filter((u) => u.startsWith('stun'));
    if (stun.length) servers.push({ urls: stun });
    if (turn.length) servers.push({ urls: turn, username: cfg.turn_user || '', credential: cfg.turn_pass || '' });
  }
  return { iceServers: servers, iceTransportPolicy: cfg.relay ? 'relay' : 'all' };
}

function urlConfig(qs) {
  const num = (k, d) => (qs.has(k) ? Number(qs.get(k)) : d);
  const def = {
    signal: qs.get('signal') || `ws://${location.hostname || '127.0.0.1'}:3536`,
    room: (qs.get('room') || '').toUpperCase(),
    key: qs.get('key') || '',
    token: qs.get('token') || undefined,
    players: num('players', 2),
    pvp: qs.has('pvp'),
    pvp_enemies: qs.has('pvp_enemies'),
    fighter: qs.get('fighter') || 'goku',
    form: num('form', undefined),
    level: num('level', undefined),
    enemy: qs.get('enemy') || 't60',
    arena: qs.get('arena') || undefined,
    count: num('count', 1),
    seed: num('seed', 1),
    delay: num('delay', 2),
    prediction: num('prediction', 12),
    pause: qs.get('pause') || 'local',
    sfx: qs.get('sfx') || 'reconcile',
    latency: num('latency', 0),
    jitter: num('jitter', 0),
    loss: num('loss', 0),
    net_seed: num('net_seed', 1),
    name: qs.get('name') || '',
    synctest: num('synctest', 8),
    ice: qs.get('ice') ?? qs.get('stun') ?? undefined,
    relay: qs.has('relay'),
    turn_user: qs.get('turn_user') || '',
    turn_pass: qs.get('turn_pass') || '',
  };
  if (!def.arena) def.arena = ENEMY_ARENA[def.enemy] || 'forest';
  return def;
}

/// Run the online part until the match starts. `api` = { x, mem(), withStr, str, canvas, showStage() }. Resolves to
/// the net handle (slot(), pump(), draw(), status) or null when refused / failed / closed.
export async function startNet(api, qs) {
  const { x, withStr, str } = api;
  document.head.append(el(`<style>${css}</style>`));
  const base = urlConfig(qs);
  const preset = qs.get('net');
  const statusBox = el('<div id="netstat" hidden></div>');
  document.body.append(statusBox);
  const withBytes = (b, f) => {
    const p = x.wf_alloc(b.length);
    new Uint8Array(api.mem().buffer, p, b.length).set(b);
    const r = f(p, b.length);
    x.wf_dealloc(p, b.length);
    return r;
  };
  let last = {};
  let room = '';
  const net = {
    cfg: base,
    peers: new Map(),
    ws: null,
    url: '',
    rtc: null,
    slot: () => Math.max(0, x.wf_net_slot()),
    status: () => last,
    pump() {
      for (let n = x.wf_net_out(); n; n = x.wf_net_out()) {
        const b = new Uint8Array(api.mem().buffer, x.wf_str_ptr(), n);
        const id = dec.decode(b.subarray(1, 1 + b[0]));
        const p = net.peers.get(id);
        if (p && p.dc.readyState === 'open') {
          try {
            p.dc.send(b.slice(1 + b[0]));
          } catch {}
        }
      }
    },
    report() {
      x.wf_net_report();
      return str();
    },
    /// the match as a .wfr replay (wf-replay format; plays in the Replays panel / buus-fury --replay)
    replay(upto = -1) {
      const n = x.wf_net_replay(upto);
      return n ? new Uint8Array(api.mem().buffer, x.wf_str_ptr(), n).slice() : null;
    },
    close() {
      for (const p of net.peers.values()) p.pc.close();
      net.peers.clear();
      if (net.ws) {
        net.ws.onclose = null;
        net.ws.close();
      }
      net.ws = null;
    },
  };
  window.wfNet = net;

  // ---- signaling (Matchbox protocol: JSON PeerEvent / PeerRequest) + WebRTC
  function connect(url) {
    net.url = url;
    const ws = new WebSocket(url);
    net.ws = ws;
    const signal = (receiver, data) => ws.readyState === 1 && ws.send(JSON.stringify({ Signal: { receiver, data } }));
    const keep = setInterval(() => (ws.readyState === 1 ? ws.send('"KeepAlive"') : ws.readyState > 1 && clearInterval(keep)), 10000);
    const iceDone = (pc) =>
      new Promise((r) => {
        if (pc.iceGatheringState === 'complete') return r();
        const f = () => pc.iceGatheringState === 'complete' && (pc.removeEventListener('icegatheringstatechange', f), r());
        pc.addEventListener('icegatheringstatechange', f);
        setTimeout(r, 3000);
      });
    function peer(id) {
      let p = net.peers.get(id);
      if (p) return p;
      const pc = new RTCPeerConnection(net.rtc);
      const dc = pc.createDataChannel('matchbox_socket_0', { negotiated: true, id: 0, ordered: false, maxRetransmits: 0 });
      dc.binaryType = 'arraybuffer';
      p = { pc, dc, pending: [], remote: false };
      net.peers.set(id, p);
      dc.onopen = () => withStr(id, (a, b) => x.wf_net_peer(a, b, 1));
      dc.onclose = () => withStr(id, (a, b) => x.wf_net_peer(a, b, 0));
      dc.onmessage = (e) => withStr(id, (ip, il) => withBytes(new Uint8Array(e.data), (dp, dl) => x.wf_net_recv(ip, il, dp, dl)));
      return p;
    }
    const addCandidates = async (p) => {
      for (const c of p.pending.splice(0)) if (c && c !== 'null') await p.pc.addIceCandidate(JSON.parse(c)).catch(() => {});
    };
    ws.onmessage = async (ev) => {
      const m = JSON.parse(ev.data);
      if (m.IdAssigned) withStr(m.IdAssigned, (p, n) => x.wf_net_set_id(p, n));
      else if (m.NewPeer) {
        // we are the offerer for this pair
        const p = peer(m.NewPeer);
        await p.pc.setLocalDescription(await p.pc.createOffer());
        await iceDone(p.pc);
        signal(m.NewPeer, { Offer: p.pc.localDescription.sdp });
      } else if (m.PeerLeft) {
        const p = net.peers.get(m.PeerLeft);
        if (p) {
          p.pc.close();
          net.peers.delete(m.PeerLeft);
          withStr(m.PeerLeft, (a, b) => x.wf_net_peer(a, b, 0));
        }
      } else if (m.Signal) {
        const { sender, data } = m.Signal;
        const p = peer(sender);
        if (data.Offer) {
          await p.pc.setRemoteDescription({ type: 'offer', sdp: data.Offer });
          p.remote = true;
          await addCandidates(p);
          await p.pc.setLocalDescription(await p.pc.createAnswer());
          await iceDone(p.pc);
          signal(sender, { Answer: p.pc.localDescription.sdp });
        } else if (data.Answer) {
          await p.pc.setRemoteDescription({ type: 'answer', sdp: data.Answer });
          p.remote = true;
          await addCandidates(p);
        } else if (data.IceCandidate) {
          p.pending.push(data.IceCandidate);
          if (p.remote) await addCandidates(p);
        }
      }
    };
    let opened = false;
    ws.onopen = () => (opened = true);
    ws.onclose = () => {
      const why = opened ? 'the signaling server closed the connection' : `cannot reach the signaling server (or it refused: wrong room token / server key, room full): ${net.cfg.signal}`;
      withStr(why, (p, n) => x.wf_net_closed(p, n));
    };
  }

  /// wf_net_lobby + connect; resolves to true / false (refused / error) once the match starts.
  async function openLobby(cfg) {
    net.cfg = cfg;
    if (cfg.mode !== 'synctest') net.rtc = await iceConfig(cfg);
    if (withStr(JSON.stringify(cfg), (p, n) => x.wf_net_lobby(p, n)) !== 0) return { error: str() };
    let info = {};
    try {
      info = JSON.parse(str());
    } catch {}
    room = info.invite || '';
    if (cfg.mode === 'synctest') return { ok: true };
    connect(info.url);
    return { ok: true, info };
  }

  let lastDraw = 0;
  net.reconnecting = false;
  net.draw = () => {
    const now = performance.now();
    if (now - lastDraw < 250) return;
    lastDraw = now;
    x.wf_net_status();
    try {
      last = JSON.parse(str());
    } catch {}
    window.wfNetStatus = last;
    const s = last;
    // keep the rejoin ticket while the match runs (a reload offers "Rejoin last match")
    if (s.ticket && s.outcome === 'running') store.set('wf.net.ticket', s.ticket);
    if (s.outcome && s.outcome !== 'running' && s.outcome !== '') store.set('wf.net.ticket', null);
    // lost everyone: a new signaling connection, the match rejoins in place
    if (s.need_transport && !net.reconnecting) {
      net.reconnecting = true;
      const url = net.url;
      net.close();
      x.wf_net_reconnect();
      setTimeout(() => {
        connect(url);
        net.reconnecting = false;
      }, 500);
    }
    if (statusBox.hidden) return;
    const rtt = (s.rtt || []).map(([, r]) => Math.round(r)).join('/');
    let t;
    if (s.phase === 'running' || s.phase === 'spectating' || s.phase === 'synctest' || s.phase === 'reconnecting')
      t = `${s.phase === 'running' ? `P${(s.local_slot ?? 0) + 1}` : s.phase.toUpperCase()}  room ${room}  rtt ${rtt || '-'} ms  frame ${s.frame}\nrollbacks ${s.stats?.rollbacks ?? 0} (max ${s.stats?.max_rollback ?? 0})  stalls ${s.stats?.stalls ?? 0}  checks ${s.stats?.hash_checks ?? 0}`;
    else t = `ROOM ${room}  ${s.phase || 'connecting'}  ${s.players_present || 1}/${s.want_players || 2} players`;
    statusBox.textContent = t;
    for (const line of [s.rejoin, s.paused_by?.length ? `PAUSED BY ${s.paused_by.map((k) => `P${k + 1}`).join(' ')}` : null].filter(Boolean)) statusBox.append(el(`<div class="bad">${line}</div>`));
    if (s.desync) {
      const d = el(`<div class="bad">DESYNC ${s.desync} </div>`);
      const a = el('<a href="#">download report</a>');
      a.onclick = (e) => {
        e.preventDefault();
        const url = URL.createObjectURL(new Blob([net.report()], { type: 'application/json' }));
        Object.assign(document.createElement('a'), { href: url, download: `desync-${room}-f${s.frame}.json` }).click();
        const r = net.replay();
        if (r) Object.assign(document.createElement('a'), { href: URL.createObjectURL(new Blob([r])), download: `desync-${room}-f${s.frame}.wfr` }).click();
      };
      d.append(a);
      statusBox.append(d);
    }
    for (const p of s.disconnected || []) statusBox.append(el(`<div class="bad">${p} disconnected</div>`));
    for (const p of s.rejoined || []) statusBox.append(el(`<div>${p} rejoined</div>`));
    if (s.phase === 'running' || s.phase === 'spectating') {
      const a = el('<a href="#">save replay</a>');
      a.onclick = (e) => {
        e.preventDefault();
        const r = net.replay();
        if (r) Object.assign(document.createElement('a'), { href: URL.createObjectURL(new Blob([r])), download: `online-${room}.wfr` }).click();
      };
      statusBox.append(document.createTextNode('\n'), a);
    }
    if (s.outcome && s.outcome !== 'running') statusBox.append(el(`<div class="bad">${s.winner != null ? `PLAYER ${s.winner + 1} WINS` : s.outcome.toUpperCase()}</div>`));
  };

  /// poll the lobby until it starts (1) or fails (2)
  const waitStart = (onTick) =>
    new Promise((resolve) => {
      const t = setInterval(() => {
        if (!net.ws && net.cfg.mode !== 'synctest') return;
        const r = x.wf_net_poll();
        net.pump();
        lastDraw = 0;
        net.draw();
        if (onTick) onTick(r);
        if (r === 1 || r === 2) {
          clearInterval(t);
          resolve(r === 1);
        }
      }, 16);
      net.stopWait = () => {
        clearInterval(t);
        resolve(null);
      };
    });

  const showStatus = () => (statusBox.hidden = false);

  // ---- URL presets (scripts / tests): no menu
  if (['host', 'join', 'spectate', 'synctest', 'rejoin'].includes(preset)) {
    const cfg = { ...base, mode: preset, secret: secret() };
    if (preset === 'rejoin') cfg.ticket = store.get('wf.net.ticket');
    const r = await openLobby(cfg);
    showStatus();
    if (r.error) {
      alert(`Online: ${r.error}`);
      return null;
    }
    if (preset === 'synctest') {
      net.draw();
      return net;
    }
    const ok = await waitStart();
    if (!ok) {
      statusBox.textContent = `Refused: ${last.error || last.outcome || 'unknown'}`;
      return null;
    }
    return net;
  }

  // ---- the in-game lobby menu (drawn by the wasm with the game's font)
  api.showStage();
  let prefs = {};
  try {
    prefs = JSON.parse(store.get('wf.net.prefs') || '{}');
  } catch {}
  if (qs.get('signal')) prefs.signal = base.signal;
  if (!prefs.signal) prefs.signal = base.signal;
  if (withStr(JSON.stringify(prefs), (p, n) => x.wf_menu_open(p, n, store.get('wf.net.ticket') ? 1 : 0)) !== 0) {
    alert(`Online: ${str()}`);
    return null;
  }
  const canvas = api.canvas;
  const g = canvas.getContext('2d', { alpha: false });
  let menuOn = true;
  const drawMenu = () => {
    if (!menuOn) return;
    const dpr = window.devicePixelRatio || 1;
    const W = Math.floor(innerWidth * dpr);
    const H = Math.floor(innerHeight * dpr);
    if (x.wf_menu_frame(W, H)) {
      const w = x.wf_menu_fb_w();
      const h = x.wf_menu_fb_h();
      const s = x.wf_menu_fb_scale();
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      canvas.style.width = `${(w * s) / dpr}px`;
      canvas.style.height = `${(h * s) / dpr}px`;
      g.putImageData(new ImageData(new Uint8ClampedArray(api.mem().buffer, x.wf_menu_fb_ptr(), w * h * 4), w, h), 0, 0);
    }
    requestAnimationFrame(drawMenu);
  };
  requestAnimationFrame(drawMenu);
  const onKey = (e) => {
    if (!menuOn) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    withStr(e.code, (p, n) => x.wf_menu_key(p, n));
    if (e.key && e.key.length === 1) withStr(e.key, (p, n) => x.wf_menu_text(p, n));
  };
  addEventListener('keydown', onKey, true);
  const menuShow = (what, msg = '') => withStr(msg, (p, n) => x.wf_menu_show(what, p, n));

  return new Promise((resolve) => {
    let inRoom = false;
    const leave = () => {
      if (net.stopWait) net.stopWait();
      net.close();
      x.wf_net_leave();
      inRoom = false;
    };
    const finish = (v) => {
      menuOn = false;
      removeEventListener('keydown', onKey, true);
      clearInterval(poll);
      menuShow(5);
      resolve(v);
    };
    const enter = async (cfg, what) => {
      leave();
      const r = await openLobby({ ...cfg, ui: true, secret: secret() });
      if (r.error) return menuShow(4, r.error);
      inRoom = true;
      menuShow(what);
      const ok = await waitStart();
      if (ok === null) return; // left
      inRoom = false;
      if (ok) {
        showStatus();
        finish(net);
      } else {
        const why = last.error || last.outcome || 'refused';
        leave();
        menuShow(4, why);
      }
    };
    const fromPrefs = (p, mode, code) => ({
      ...base,
      mode,
      room: code || '',
      signal: p.signal || base.signal,
      players: p.players,
      pvp: p.mode !== 0,
      pvp_enemies: p.mode === 2,
      fighter: p.fighter,
      form: p.form,
      enemy: p.enemy,
      arena: ENEMY_ARENA[p.enemy] || 'forest',
      count: p.count || 1,
      delay: p.delay,
      pause: p.pause,
      name: p.name,
      seed: Math.floor(Math.random() * 99999) + 1,
    });
    const poll = setInterval(() => {
      for (let n = x.wf_menu_action(); n; n = x.wf_menu_action()) {
        let a;
        try {
          a = JSON.parse(str());
        } catch {
          continue;
        }
        if (a.prefs) {
          prefs = a.prefs;
          store.set('wf.net.prefs', JSON.stringify(prefs));
        }
        if (a.t === 'host') enter(fromPrefs(prefs, 'host'), 1);
        else if (a.t === 'join') enter(fromPrefs(prefs, a.spectate ? 'spectate' : 'join', a.code), a.spectate ? 3 : 2);
        else if (a.t === 'rejoin') enter({ ...fromPrefs(prefs, 'rejoin'), ticket: store.get('wf.net.ticket') }, 2);
        else if (a.t === 'ready') x.wf_net_ready(a.on ? 1 : 0);
        else if (a.t === 'fighter' && inRoom) withStr(JSON.stringify({ fighter: a.fighter, form: a.form }), (p, l) => x.wf_net_fighter(p, l));
        else if (a.t === 'start') x.wf_net_start();
        else if (a.t === 'leave') leave();
        else if (a.t === 'close') {
          leave();
          finish(null);
          // no match: back to the local game
          location.search = '';
        }
      }
      if (inRoom) net.pump();
    }, 16);
  });
}
