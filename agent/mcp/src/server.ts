// MCP server (stdio): lets an LLM agent play the Buu's Fury battle simulator. Tools are thin wrappers over game.ts.
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { z } from 'zod'
import * as G from './game'
import type { StopOn } from './macros'

const server = new McpServer(
  { name: 'buu-battle', version: '0.1.0' },
  {
    instructions:
      "Play Dragon Ball Z: Buu's Fury battles (a frame-exact simulator of the GBA game). new_battle -> session id. The game is paused between calls; " +
      'every action tool runs N frames (60 frames = 1 s) and returns {result, observation}. Positions are world pixels (x right, y down) of the ' +
      'feet (footprint centre). Typical loop: observe -> plan -> act 10-60 frames -> repeat; save_state/rewind to explore; run_controller for reflex tasks.',
  },
)

const sid = z.string().describe('session id from new_battle (e.g. "s1")')
const stopOn = z
  .array(z.enum(['hurt', 'kill', 'hit', 'phase']))
  .optional()
  .describe('end the action early: hurt = you lost HP, kill = an enemy was defeated, hit = you dealt a hit, phase = a new wave spawned')

type Out = unknown
function ok(v: Out) {
  return { content: [{ type: 'text' as const, text: JSON.stringify(v) }] }
}
function wrap<A>(fn: (a: A) => Out) {
  return async (a: A) => {
    try {
      return ok(fn(a))
    } catch (e) {
      return { content: [{ type: 'text' as const, text: `error: ${(e as Error).message}` }], isError: true }
    }
  }
}

server.registerTool(
  'list_fighters',
  { description: 'Playable fighters with their forms and move lists (A melee, R block, B = selected skill, L cycles skills).', inputSchema: {} },
  wrap(() => G.listFighters()),
)

server.registerTool(
  'list_enemies',
  {
    description: 'Search the 150 enemy types / bosses (and story groups). Returns ids usable in new_battle(enemy=...).',
    inputSchema: { search: z.string().optional().describe('name substring, e.g. "dabura", "janemba", "saibaman"'), group: z.string().optional().describe('Boss | Enemy | Story group | Helper'), limit: z.number().int().optional() },
  },
  wrap((a: { search?: string; group?: string; limit?: number }) => G.listEnemies(a.search, a.group, a.limit ?? 25)),
)

server.registerTool('list_arenas', { description: 'Arenas (real game maps with collision). bounds = walkable bounding box [x0,y0,x1,y1].', inputSchema: {} }, wrap(() => G.listArenas()))

server.registerTool('list_presets', { description: 'Story fights (multi-phase boss chains) usable as new_battle(preset=...).', inputSchema: {} }, wrap(() => G.listPresets()))

server.registerTool(
  'new_battle',
  {
    description:
      'Start a battle; returns the session id and a full first observation (skills, arena ASCII map). Multiple sessions can run side by side. ' +
      'Deterministic: same config + same inputs = same result.',
    inputSchema: {
      fighter: z.string().optional().describe('goku (default), gohan, goten, trunks, vegeta, gotenks, vegito, gogeta'),
      form: z.number().int().optional().describe('form id from list_fighters (default: base form)'),
      level: z.number().int().optional().describe('50..200, default 80'),
      enemy: z.string().optional().describe('enemy id (t18) or name ("Dabura"); ignored with preset'),
      count: z.number().int().optional().describe('copies of the enemy (1..12)'),
      preset: z.string().optional().describe('story fight id from list_presets (e.g. dabura, janemba, majin_fighters)'),
      arena: z.string().optional().describe("arena id (default: the enemy's own map, else crater)"),
      seed: z.number().int().optional().describe('RNG seed (default 1)'),
      skill_level: z.number().int().optional().describe('0..2 (default 2 = Lv3 skills)'),
      stat_spend: z.string().optional().describe('even (default) | STR | POW | END | SPD | none'),
    },
  },
  wrap((a: G.NewBattleArgs) => G.newBattle(a)),
)

server.registerTool('list_sessions', { description: 'Open sessions.', inputSchema: {} }, wrap(() => G.listSessions()))
server.registerTool('close_session', { description: 'Drop a session.', inputSchema: { session: sid } }, wrap((a: { session: string }) => G.closeSession(a.session)))

server.registerTool(
  'observe',
  {
    description:
      'Current state: player (pos, facing, hp/ep, state, can_act, selected skill), enemies sorted by distance (id, pos, hp, dist, dir, ai = what its AI runs now, ' +
      'melee = "in_range" if a punch now would hit / "turn_<dir>" if turning first would), projectiles (team, pos, vel px/frame, eta = frames until it hits you), ' +
      'walls = free px in each direction, events/sfx since the last observation. map=true adds an ASCII map.',
    inputSchema: { session: sid, map: z.boolean().optional(), full: z.boolean().optional().describe('include skill list / stats / arena') },
  },
  wrap((a: { session: string; map?: boolean; full?: boolean }) => G.doObserve(a.session, a)),
)

server.registerTool(
  'screenshot',
  {
    description: 'PNG of the battle drawn by the game renderer (sprites, effects, HUD). Enemies are labelled with their ids. view: gba (240x160), wide (400x256, default) or arena (whole map).',
    inputSchema: { session: sid, view: z.enum(['gba', 'wide', 'arena']).optional(), scale: z.number().int().optional().describe('1..4, default 2'), labels: z.boolean().optional() },
  },
  async (a: { session: string; view?: 'gba' | 'wide' | 'arena'; scale?: number; labels?: boolean }) => {
    try {
      const png = G.doScreenshot(a.session, a.view ?? 'wide', a.scale ?? 2, a.labels ?? true)
      return { content: [{ type: 'image' as const, data: png.toString('base64'), mimeType: 'image/png' }] }
    } catch (e) {
      return { content: [{ type: 'text' as const, text: `error: ${(e as Error).message}` }], isError: true }
    }
  },
)

server.registerTool(
  'press',
  {
    description:
      'Low level: hold buttons for N frames (released afterwards only if the next call holds nothing). Buttons: A (punch), B (skill), R (block), L (cycle skill), ' +
      'UP/DOWN/LEFT/RIGHT, joined with "+", e.g. "RIGHT+A". A press acts on the frame the button goes down: to punch twice, release in between. ' +
      'steps runs a sequence, e.g. [{"buttons":"A","frames":1},{"buttons":"","frames":16},{"buttons":"A","frames":1}].',
    inputSchema: {
      session: sid,
      buttons: z.string().optional(),
      frames: z.number().int().optional().describe('default 1'),
      steps: z.array(z.object({ buttons: z.string().optional(), frames: z.number().int().optional() })).optional(),
      stop_on: stopOn,
    },
  },
  wrap((a: { session: string; buttons?: string; frames?: number; steps?: { buttons?: string; frames?: number }[]; stop_on?: StopOn[] }) => G.press(a.session, a)),
)

server.registerTool(
  'wait',
  { description: 'Do nothing for N frames (EP regenerates slowly).', inputSchema: { session: sid, frames: z.number().int(), stop_on: stopOn } },
  wrap((a: { session: string; frames: number; stop_on?: StopOn[] }) => G.wait(a.session, a.frames, a.stop_on)),
)

server.registerTool(
  'move_to',
  {
    description: 'Walk to (x, y) (feet position, as in observations) along a collision-aware path. Stops on arrival, max_frames (default 600) or stop_on (default ["hurt"]).',
    inputSchema: { session: sid, x: z.number(), y: z.number(), max_frames: z.number().int().optional(), stop_on: stopOn },
  },
  wrap((a: { session: string; x: number; y: number; max_frames?: number; stop_on?: StopOn[] }) => G.moveTo(a.session, a.x, a.y, a.max_frames, a.stop_on ?? ['hurt'])),
)

server.registerTool(
  'face',
  { description: 'Turn toward an enemy (target id / name, default nearest) or a direction (down/up/left/right) without moving.', inputSchema: { session: sid, target: z.string().optional(), direction: z.string().optional() } },
  wrap((a: { session: string; target?: string; direction?: string }) => G.face(a.session, a.target, a.direction)),
)

server.registerTool(
  'attack',
  {
    description:
      'Melee macro: walk to a punching spot next to the target, turn, and punch whenever able (one punch per press, 17-frame recovery). ' +
      'Ends when the target is defeated, after `hits` damaging hits, max_frames (default 300) or stop_on.',
    inputSchema: { session: sid, target: z.string().optional().describe('enemy id ("12") or name; default nearest'), hits: z.number().int().optional(), max_frames: z.number().int().optional(), stop_on: stopOn },
  },
  wrap((a: { session: string; target?: string; hits?: number; max_frames?: number; stop_on?: StopOn[] }) => G.attack(a.session, a.target, a.hits, a.max_frames, a.stop_on)),
)

server.registerTool(
  'use_skill',
  {
    description:
      'Select a skill (cycles L), optionally line up with / face a target, then hold B for charge_frames (taps times), then wait until you can act. ' +
      'Energy Blast: taps=N charge_frames=1. Beams (Kamehameha, Final Flash...): hold 30-120 frames to sustain. Big Bang / Masenko / Ghosts: charge then release. ' +
      'Super Saiyan: charge_frames=1 (needs EP 10 and no cooldown; drains a gauge, reverts when empty).',
    inputSchema: {
      session: sid,
      skill: z.string().describe('skill name ("Kamehameha") or slot number'),
      charge_frames: z.number().int().optional().describe('frames B is held per tap (default 1)'),
      taps: z.number().int().optional(),
      gap: z.number().int().optional().describe('frames between taps (default 26; Energy Blast refires only about every 25 frames, shorter gaps drop shots)'),
      target: z.string().optional(),
      align: z.boolean().optional().describe('first walk onto the target row/column so a straight shot connects'),
      steer: z.string().optional().describe('d-pad held while charging (Flame Kamehameha whip, Dragon Fist head, Energy Sword move)'),
      max_frames: z.number().int().optional(),
      stop_on: stopOn,
    },
  },
  wrap((a: Parameters<typeof G.useSkill>[1] & { session: string }) => G.useSkill(a.session, a)),
)

server.registerTool(
  'block',
  { description: 'Hold R: frontal hits do 1/16 damage. shield=true also holds B: energy shield (all damage 0 once charged, drains EP).', inputSchema: { session: sid, frames: z.number().int(), shield: z.boolean().optional(), stop_on: stopOn } },
  wrap((a: { session: string; frames: number; shield?: boolean; stop_on?: StopOn[] }) => G.block(a.session, a.frames, a.shield, a.stop_on)),
)

server.registerTool(
  'dash',
  { description: 'Double-tap dash in a direction for N frames (~2.5x walk speed; dashing into a wall stuns you briefly).', inputSchema: { session: sid, direction: z.string(), frames: z.number().int().optional(), stop_on: stopOn } },
  wrap((a: { session: string; direction: string; frames?: number; stop_on?: StopOn[] }) => G.dash(a.session, a.direction, a.frames, a.stop_on)),
)

server.registerTool(
  'save_state',
  { description: 'Name the current moment (cheap; up to 64 per session).', inputSchema: { session: sid, name: z.string() } },
  wrap((a: { session: string; name: string }) => G.saveState(a.session, a.name)),
)
server.registerTool(
  'load_state',
  { description: 'Go back to a saved moment (the inputs after it are discarded: a new branch).', inputSchema: { session: sid, name: z.string() } },
  wrap((a: { session: string; name: string }) => G.loadState(a.session, a.name)),
)
server.registerTool(
  'rewind',
  { description: 'Undo the last N frames (any amount, back to frame 0).', inputSchema: { session: sid, frames: z.number().int() } },
  wrap((a: { session: string; frames: number }) => G.rewind(a.session, a.frames)),
)

server.registerTool(
  'get_replay',
  { description: 'Config + run-length encoded input log + final state hash of a session (feed to play_replay).', inputSchema: { session: sid } },
  wrap((a: { session: string }) => G.getReplay(a.session)),
)
server.registerTool(
  'play_replay',
  {
    description: 'Re-simulate a replay from scratch in a new session and check that the final state hash matches (determinism check).',
    inputSchema: { replay: z.string().describe('the JSON returned by get_replay'), keep: z.boolean().optional().describe('keep the new session open (default true)') },
  },
  wrap((a: { replay: string; keep?: boolean }) => G.playReplay(JSON.parse(a.replay), a.keep ?? true)),
)

server.registerTool(
  'run_controller',
  {
    description:
      'Run your own policy every frame: code is a JS function (obs, mem) => buttons, where obs has the observe() shape (player, enemies, projectiles, walls, frame) ' +
      'and mem is a persistent object. Return a button string ("LEFT+A"), an array of names, a mask number, or null/"STOP" to end. ' +
      'Helpers: BTN, toward(fromPos, toPos, dead=2) -> d-pad names, dist(a, b), log(...). Sandboxed (no I/O), 50 ms per call. ' +
      'Remember presses act on the button-down edge: alternate A with "" to punch repeatedly.',
    inputSchema: {
      session: sid,
      code: z.string(),
      frames: z.number().int().describe('max frames to run (<= 36000)'),
      every: z.number().int().optional().describe('call the function every N frames and hold its answer in between (default 1)'),
      stop_on: stopOn,
      timeout_ms: z.number().int().optional(),
    },
  },
  wrap((a: { session: string; code: string; frames: number; every?: number; stop_on?: StopOn[]; timeout_ms?: number }) => G.runController(a.session, a)),
)

await server.connect(new StdioServerTransport())
