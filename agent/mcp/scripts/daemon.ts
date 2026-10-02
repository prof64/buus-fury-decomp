// Play-test bridge: spawns the MCP server over stdio with the official MCP client and exposes it on
// http://127.0.0.1:$PORT (default 7781) so a shell can make one tool call at a time against long-lived sessions:
//   node dist/scripts/call.mjs observe '{"session":"s1"}'
// Images are written to ./shots/<n>.png and replaced by their path. Every call is appended to ./shots/calls.log.
import { createServer } from 'node:http'
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')
const shots = resolve(root, 'shots')
mkdirSync(shots, { recursive: true })
const client = new Client({ name: 'play-bridge', version: '0.1.0' })
await client.connect(new StdioClientTransport({ command: process.execPath, args: [resolve(root, 'dist/src/server.mjs')], stderr: 'inherit' }))
let n = 0
const port = Number(process.env.PORT ?? 7781)
createServer(async (req, res) => {
  let body = ''
  for await (const c of req) body += c
  try {
    const { tool, args } = JSON.parse(body || '{}')
    let text: string
    if (tool === 'tools/list') {
      const t = await client.listTools()
      text = JSON.stringify(t.tools.map((x) => ({ name: x.name, description: x.description, input: x.inputSchema })), null, 1)
    } else {
      const r = await client.callTool({ name: tool, arguments: args ?? {} })
      const parts: string[] = []
      for (const c of r.content as { type: string; text?: string; data?: string }[]) {
        if (c.type === 'text') parts.push(c.text!)
        else if (c.type === 'image') {
          const p = resolve(shots, `${++n}.png`)
          writeFileSync(p, Buffer.from(c.data!, 'base64'))
          parts.push(`[image ${p}]`)
        }
      }
      text = parts.join('\n')
      appendFileSync(resolve(shots, 'calls.log'), `${tool} ${JSON.stringify(args)} -> ${text.length} chars\n`)
    }
    res.end(text)
  } catch (e) {
    res.statusCode = 500
    res.end(String((e as Error).stack))
  }
}).listen(port, '127.0.0.1', () => console.error(`bridge on http://127.0.0.1:${port}`))
