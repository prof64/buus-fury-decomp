# buu-battle - Claude Code plugin

Lets Claude (or any MCP client) play Dragon Ball Z: Buu's Fury headlessly on the deterministic Rust core
(`engine/crates/wf-core`): the `buu-battle` MCP server (`engine/crates/wf-mcp`) plus the `buu-battle` skill that
explains controls, mechanics, the observation and how to write Rhai controllers.

```
agent/plugin/
  .claude-plugin/plugin.json      plugin manifest
  .mcp.json                       MCP server: ${CLAUDE_PLUGIN_ROOT}/bin/buu-mcp (stdio)
  bin/buu-mcp                     launcher: finds the wf-mcp binary and the ROM
  skills/buu-battle/SKILL.md      the skill
  skills/buu-battle/controllers/  example Rhai controllers (melee.rhai, yakon.rhai)
agent/.claude-plugin/marketplace.json   a one-plugin local marketplace (for /plugin install)
```

## Install

1. **Build the server** (a recent stable Rust; no other toolchain):
   ```
   cd engine && cargo build --release -p wf-mcp
   ```
   The binary is `engine/target/release/wf-mcp`; the launcher finds it relative to the plugin directory. If the
   plugin lives elsewhere (e.g. installed from a remote marketplace, which copies it), put the binary on PATH
   (`cargo install --path engine/crates/wf-mcp`) or set `WF_MCP=/abs/path/to/wf-mcp`.
2. **Content**: the server needs your own ROM (Buu's Fury USA, `roms/buu.gba` in this repository, or
   `WF_ROM=/path/to/buu.gba`). The first start extracts the content (~5 s) into `~/Library/Caches/wf-buu/agent`
   (Linux: `~/.cache/wf-buu/agent`, override with `WF_CACHE`); later starts read the cache. Check with
   `engine/target/release/wf-mcp --check`. Nothing ROM-derived is shipped.
3. **Load the plugin in Claude Code** - pick one:
   - for a session, in place (no copy, no global config change): `claude --plugin-dir agent/plugin`
   - as an installed plugin from the local marketplace in `agent/` (read in place, not copied):
     `/plugin marketplace add ./agent` then `/plugin install buu-battle@legacy-of-goku-x`
   - MCP server only, project scope: a `.mcp.json` at the repository root with
     `{"mcpServers": {"buu-battle": {"command": "agent/plugin/bin/buu-mcp"}}}`, and the skill copied or linked to
     `.claude/skills/buu-battle`.
4. Ask for a fight: "play Dabura with a Lv80 Goku", "beat Yakon using his post-combo window", "write a controller
   that clears 6 Majin Fighters at Lv70 and evaluate it on 32 seeds".

Everything runs headless and muted: the server never opens a window or an audio device.

## Other clients

- Any MCP client: run `agent/plugin/bin/buu-mcp` (or `wf-mcp`) as a stdio server.
- Scripts: `engine/crates/wf-mcp/python/mcp_client.py` (a 70-line MCP client), `playtest.py` (the validation fights).
- Fast path for bots / RL: `engine/crates/wf-mcp/python/wf_gym.py` (ctypes over `libwf_mcp`: gym-style
  `reset/step/obs`, snapshots, hashes, ~1.3 M frames/s batched) and `wf_gym.call(tool, args)` for the whole tool
  API in-process.
- Shell: `wf-mcp --call new_battle '{"preset":"dabura"}'` runs one tool call.
