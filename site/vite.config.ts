import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@data': root + 'data',
      '@re': root + 're',
      '@tools': root + 'tools',
    },
  },
  server: { fs: { allow: [root] }, port: 5317 },
})
