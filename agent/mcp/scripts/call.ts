// One MCP tool call through the play-test bridge (scripts/daemon.ts): node dist/scripts/call.mjs <tool> '<json>'
// String values "@file:<path>" are replaced by the file's contents (controller code). --pretty indents the answer.
import { readFileSync } from 'node:fs'

const [tool, json, flag] = process.argv.slice(2)
const port = Number(process.env.PORT ?? 7781)
const args = json ? JSON.parse(json) : {}
for (const [k, v] of Object.entries(args)) if (typeof v === 'string' && v.startsWith('@file:')) args[k] = readFileSync(v.slice(6), 'utf8')
const r = await fetch(`http://127.0.0.1:${port}/`, { method: 'POST', body: JSON.stringify({ tool, args }) })
const t = await r.text()
if (flag === '--pretty') {
  try {
    console.log(JSON.stringify(JSON.parse(t), null, 1))
  } catch {
    console.log(t)
  }
} else console.log(t)
