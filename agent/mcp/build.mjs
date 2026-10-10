// Bundles the MCP server (and the play-test scripts) with the simulator sources from ../../site/src.
// The sim is imported unmodified; two browser-only bits are replaced at build time:
//   site/src/data.ts        uses Vite's import.meta.glob + React -> stub (we build the tables from the JSON files)
//   import.meta.env.BASE_URL (assets.ts)                         -> "" (images are resolved by our fake Image)
import { build } from 'esbuild'
import { readdirSync } from 'node:fs'

const stubData = {
  name: 'stub-site-data',
  setup(b) {
    b.onResolve({ filter: /^\.\.\/\.\.\/\.\.\/data$/ }, (args) => (args.importer.includes('/rulesets/buu/') ? { path: 'site-data-stub', namespace: 'stub' } : undefined))
    b.onLoad({ filter: /.*/, namespace: 'stub' }, () => ({
      contents: 'export function loadData() { throw new Error("loadData is browser-only; the MCP server builds BuuData itself") }',
      loader: 'js',
    }))
  },
}

const entries = ['src/server.ts', ...readdirSync('scripts').filter((f) => f.endsWith('.ts')).map((f) => 'scripts/' + f)]
await build({
  entryPoints: entries,
  outdir: 'dist',
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node24',
  sourcemap: 'inline',
  outExtension: { '.js': '.mjs' },
  packages: 'external',
  define: { 'import.meta.env.BASE_URL': '""' },
  plugins: [stubData],
  logLevel: 'warning',
  banner: { js: "import { createRequire as __cr } from 'node:module'; const require = __cr(import.meta.url);" },
})
console.log('built', entries.join(', '))
