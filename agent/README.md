# agent/ - LLM agents playing the Buu's Fury battle simulator

**Current version: the Rust core.** `plugin/` is the Claude Code plugin (MCP server `engine/crates/wf-mcp` + the
`buu-battle` skill + example Rhai controllers); install steps in [plugin/README.md](plugin/README.md), design,
performance and play results in [DESIGN-rust.md](DESIGN-rust.md). Everything below describes the earlier
TypeScript prototype (kept for reference).

Prototype of an agent interface (MCP server + Claude Code skill) over the TypeScript simulator in
`site/src/battle/`, built to learn what works before the interface is rebuilt on the Rust core. Design notes and
play-test findings: [DESIGN.md](DESIGN.md).

```
agent/
  mcp/                 MCP server (TypeScript, Node 24, @modelcontextprotocol/sdk, stdio)
    src/server.ts      tool schemas -> src/game.ts (the transport-free API)
    src/session.ts     battle session: input log, events, save/load/rewind by re-simulation, replay + hash
    src/observe.ts     observation builder, ASCII map, enemy damage profile
    src/macros.ts      move_to (BFS on the walk grid), attack, use_skill, face, dash, block (deterministic)
    src/controller.ts  run_controller: agent JS policy in node:vm
    src/canvas.ts      software Canvas 2D + fake Image/document so the game's own renderer runs headless
    src/render.ts      screenshot = World.draw + HUD through that canvas -> PNG
    scripts/           daemon.ts + call.ts (shell bridge for play-testing), selftest.ts, ctl_*.js controllers
  skill/SKILL.md       Claude Code skill: how to play through the tools
  mcp.example.json     example MCP registration
```

The simulator sources are imported from `../../site/src` unmodified; esbuild bundles them and swaps the two
browser-only pieces (`site/src/data.ts` -> stub, `import.meta.env.BASE_URL` -> ""). Data comes from `data/buu`,
`data/battle`; screenshots read `site/public/assets/buu` (generated locally by `tools/extract`, not in git).

## Run

```
cd agent/mcp
npm install
npm run build          # -> dist/src/server.mjs (+ dist/scripts/*)
npm run typecheck
npm run selftest       # plays the validation fights in-process, checks replay determinism
```

## Add it to Claude Code

Project scope: copy `agent/mcp.example.json` to `.mcp.json` at the repository root (the paths are relative to it),
or run once:

```
claude mcp add buu-battle -e BUU_REPO=$PWD -- node $PWD/agent/mcp/dist/src/server.mjs
```

`BUU_REPO` is optional (the server finds the repo by walking up from its own file). To use the skill, copy or
symlink `agent/skill` to `.claude/skills/buu-battle`.

## Play from a shell (what the play tests used)

```
node agent/mcp/dist/scripts/daemon.mjs &          # MCP client <-> server over stdio, HTTP on 127.0.0.1:7781
node agent/mcp/dist/scripts/call.mjs new_battle '{"fighter":"goku","level":80,"preset":"dabura"}'
node agent/mcp/dist/scripts/call.mjs attack '{"session":"s1","stop_on":["hurt"]}'
node agent/mcp/dist/scripts/call.mjs run_controller '{"session":"s1","code":"@file:agent/mcp/scripts/ctl_melee.js","frames":3000}'
node agent/mcp/dist/scripts/call.mjs screenshot '{"session":"s1"}'   # -> agent/mcp/shots/<n>.png
```
