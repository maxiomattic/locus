import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { mediaPlugin } from './server/media.mjs'
export default defineConfig({
  plugins: [react(), mediaPlugin()],
  build: { target: 'es2022' },
  server: { allowedHosts: ['surface.tail441548.ts.net'], port: 5180 },
})
