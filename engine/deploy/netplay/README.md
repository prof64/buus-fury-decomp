# Online play over the internet: deploy recipe

Nothing here is deployed. These files describe how to put the two small servers online play needs on the
internet. Game traffic is peer to peer (WebRTC). The servers only introduce peers and, when a direct path is
impossible, relay packets.

| piece | what it does | ports | load |
|---|---|---|---|
| **wf-signal** (behind Caddy) | rooms by code, room tokens, optional server key, WebRTC handshake relay, `GET /ice` = STUN/TURN list with time-limited TURN credentials | 443/tcp (wss/https), 80/tcp (certificates) | tiny: a few KB per match |
| **TURN** (coturn, or `wf-turn`) | STUN (finds your public address) + relay for players behind symmetric NATs / strict firewalls (~10–20 % of home connections, more on mobile / corporate networks) | 3478/udp+tcp, 49160–49200/udp | only relayed matches: ~130 B × 60/s ≈ 8 KB/s each way per relayed player (~30 MB per hour) |

Clients:

* Native: open the lobby with `buus-fury --online` (or F12 in game) and set **Server** to `wss://play.example.com`.
  You can also skip the menu: `buus-fury --net-host --signal wss://play.example.com [--net-key KEY]`, which prints
  an invite code like `KQZTA-7F3K2Q`. Other players run `buus-fury --net-join KQZTA-7F3K2Q --signal wss://…`.
  The client fetches ICE servers from `https://play.example.com/ice` by itself (`--net-ice auto` is the default
  for non-local servers).
* Browser: `/app/?net&signal=wss://play.example.com[&key=KEY]` opens the in-game lobby. The page and the signaling
  server may be on different origins (`/ice` sends CORS headers).

## Option A: one VPS with Docker (everything included)

Any Linux VPS with a public IPv4 works: Hetzner CX22 (~€4/month), DigitalOcean / Vultr / Linode (~$5–6/month),
Oracle free tier. 1 vCPU / 1 GB RAM is plenty. Traffic is the only thing that grows. Most plans include 1–20 TB,
which is far more than relayed matches need.

1. Point a DNS A record (for example `play.example.com`) at the server.
2. Open the firewall: 80/tcp, 443/tcp, 3478/udp, 3478/tcp, 49160–49200/udp.
3. Install Docker, copy the `engine/` workspace (or clone the repo), then:
   ```sh
   cd engine/deploy/netplay
   cp env.example .env   # set WF_DOMAIN, PUBLIC_IP, TURN_SECRET (openssl rand -hex 32), optionally WF_SIGNAL_KEY
   docker compose up -d --build
   curl https://play.example.com/healthz             # ok
   curl "https://play.example.com/ice?key=KEY"       # {"iceServers":[stun…, turn… with username/credential]}
   ```
4. Check the relay from a player machine. This forces the TURN path the way two symmetric NATs would:
   `buus-fury --headless --net-host --signal wss://play.example.com --net-relay-only --frames 600 --net-random-input 1`
   on one machine, and `--net-join CODE-TOKEN --net-relay-only` on another. The coturn log shows the allocations.

What the compose file runs:

* `caddy` gets and renews the Let's Encrypt certificate and proxies `wss://` and `https://` to `signal:3536`.
  WebSocket upgrades pass through as they are.
* `signal` runs `wf-signal` (image built from this repo's `Dockerfile`). It requires room tokens
  (`WF_REQUIRE_TOKEN=1`), takes an optional server key, allows 8 peers per room and 2000 connections, and gives out
  TURN credentials that are valid for 24 h.
* `turn` runs coturn with host networking. It uses REST API credentials from the shared secret
  (`use-auth-secret`), so no password list is needed. It refuses to relay to private / loopback ranges and has
  per-user and total quotas. To use the small `wf-turn` instead (UDP only): `docker compose --profile wf-turn up -d`
  after removing the coturn service.
* **turns:// (TURN over TLS on 5349)** helps players on networks that block UDP. To enable it, give coturn a
  certificate (for example `certbot certonly --standalone -d turn.example.com`, then mount the files and set `cert=` /
  `pkey=` / `tls-listening-port=5349` in `turnserver.conf`). After that, add
  `turns:${WF_DOMAIN}:5349?transport=tcp` to `WF_TURN_URLS`.

Without Docker: `cargo build --release -p wf-net --features server,turn --bin wf-signal --bin wf-turn`. Run
`wf-signal 127.0.0.1:3536 --turn-url turn:play.example.com:3478 --turn-secret S --require-token` behind any TLS
proxy (Caddy, nginx). You can also use its own TLS: `--tls 0.0.0.0:443 --tls-cert fullchain.pem --tls-key
privkey.pem`. Add `wf-turn --public-ip <IP> --secret S` or coturn with `use-auth-secret` /
`static-auth-secret=S`. Example systemd unit:

```ini
[Unit]
Description=Buu's Fury signaling
After=network-online.target
[Service]
Environment=WF_TURN_SECRET=...  WF_TURN_URLS=turn:play.example.com:3478  WF_REQUIRE_TOKEN=1
ExecStart=/usr/local/bin/wf-signal 127.0.0.1:3536
DynamicUser=yes
Restart=always
[Install]
WantedBy=multi-user.target
```

## Option B: fly.io for signaling + a TURN provider

`fly.toml` runs only `wf-signal`. Fly terminates TLS, so players use `wss://<app>.fly.dev`. One shared-cpu-1x
256 MB machine costs about $2/month and must stay running, because WebSockets are long-lived. TURN on fly.io is
possible but awkward: it needs UDP on `fly-global-services`, a dedicated IPv4 (~$2/month) and a relay port range.
Use one of these for the relay instead:

* **A TURN provider.** Cloudflare Realtime TURN (pay per GB, small free allowance), Metered.ca (free tier ~50 GB/month,
  then ~$0.40/GB), Twilio Network Traversal (~$0.40/GB). They hand out credentials through their own API. Either
  put their ICE JSON in the clients (`--net-ice turn:… --net-turn-user U --net-turn-pass P`, or the web app's
  `&ice=[{"urls":…,"username":…,"credential":…}]`), or proxy them from `/ice`. That needs a small change: wf-signal
  only mints coturn-style REST credentials today.
* **The VPS recipe's `turn` service** on a $4–5 VPS, with `WF_TURN_URLS` / `WF_TURN_SECRET` set on fly.

```sh
cd engine
fly launch --no-deploy --copy-config --config deploy/netplay/fly.toml --dockerfile deploy/netplay/Dockerfile
fly secrets set WF_TURN_SECRET=... WF_SIGNAL_KEY=... --config deploy/netplay/fly.toml
fly deploy --config deploy/netplay/fly.toml --dockerfile deploy/netplay/Dockerfile
```

## Security notes

* Room codes are 5 letters. On a non-local server the host also gets a 6-character **room token**: the invite is
  `CODE-TOKEN`. The first peer fixes a room's token, and later peers must present it. With
  `WF_REQUIRE_TOKEN=1`, rooms without a token are refused.
* `WF_SIGNAL_KEY` (optional) closes the server, and its TURN quota, to anyone without the key. The native client
  uses `--net-key` / `WF_SIGNAL_KEY`. The web app uses `&key=`. It is a shared password for your group, not a user
  account system.
* TURN credentials expire (`WF_TURN_TTL`, 24 h by default) and are derived from the secret. coturn is configured
  not to relay into private address ranges.
* Rejoining a running match needs the seat secret. Only the player's own game has it (in the rejoin ticket, which
  is `<data>/net-rejoin.json` natively and localStorage in the browser). Other peers check it against the hash
  that the start message carries.
* The servers never see game content or inputs in the clear. Inputs travel in DTLS-encrypted WebRTC data
  channels, and through TURN when relayed (TURN relays the encrypted packets).
